//! Deterministic provider registry and local format detection.

mod catalog;
mod invocations;
mod models;
mod verify;

pub use catalog::{
    bundled_serial, effective_catalog, effective_models, parse_overlay, BundledSerial,
    CatalogError, CatalogOrigin, EffectiveCatalog, EffectiveModels, EffectiveProviderModels,
    Overlay, OverlayModel, OverlayProvider, MAX_CATALOG_BYTES, MAX_MODELS_PER_PROVIDER,
};

pub use invocations::{validate_invocations, Invocation, INVOCATION_KINDS};

pub use verify::{
    check_serial, embedded_public_key, evaluate_remote, verify_signature, SerialCheck, Verdict,
};

pub use models::{
    default_rules, infer_capabilities, load_rules, parse_model_list, CapabilityRule,
    CapabilitySource, DiscoveredModel, ModelPage, ParseError, CAPABILITY_VOCABULARY,
};

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::IpAddr, sync::LazyLock};
use url::Url;

/// Whether one key reaches a single vendor's models or many vendors' models.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Direct,
    Aggregator,
}

/// Wire format of the provider's model-list endpoint (`validationUrl`).
/// This is about the models-list response and request headers, not `baseUrl`:
/// Gemini's `baseUrl` is the OpenAI-compatible path, but its list endpoint is native.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Openai,
    Anthropic,
    Gemini,
}

/// Hand-annotated catalog model; `capabilities` empty means "not annotated".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Retired model: not offered in dropdowns, but still used for capability inference.
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinition {
    pub id: String,
    pub name: String,
    pub env_keys: Vec<String>,
    #[serde(rename = "keyPrefixes", default)]
    pub prefixes: Vec<String>,
    pub base_url: String,
    #[serde(default)]
    pub validation_url: Option<String>,
    /// Official page where the user creates an API key (opened in the user's browser).
    #[serde(default)]
    pub key_url: Option<String>,
    /// Env name for the base URL; callers derive `<ID>_BASE_URL` when absent.
    #[serde(default)]
    pub base_url_env_key: Option<String>,
    /// Present for AI model providers; absent for GitHub/Supabase/Cloudflare-style services.
    #[serde(default)]
    pub model_env_key: Option<String>,
    #[serde(default)]
    pub kind: ProviderKind,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub models: Vec<CatalogModel>,
    /// Set only by a remote catalog; otherwise the first non-deprecated model is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Bundled only (never from the remote overlay): capability -> how to call it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub invocations: BTreeMap<String, Invocation>,
    /// Official API docs entry (https), shown to users and AI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

/// Shared with the desktop UI and MCP server; edit the JSON, not this crate.
static REGISTRY: LazyLock<Vec<ProviderDefinition>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../packages/provider-definitions/providers.json"
    ))
    .expect("packages/provider-definitions/providers.json is valid")
});

pub fn default_registry() -> Vec<ProviderDefinition> {
    REGISTRY.clone()
}

pub fn detect_provider(value: &str, registry: &[ProviderDefinition]) -> Option<ProviderDefinition> {
    registry
        .iter()
        .filter_map(|provider| {
            provider
                .prefixes
                .iter()
                .filter(|prefix| value.starts_with(prefix.as_str()))
                .max_by_key(|prefix| prefix.len())
                .map(|prefix| (prefix.len(), provider))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, provider)| provider.clone())
}

pub fn validate_endpoint(
    provider_id: &str,
    endpoint: &str,
    registry: &[ProviderDefinition],
) -> Result<Url, String> {
    let url = check_https_public(endpoint)?;
    let host = url.host_str().unwrap_or_default();
    let expected = registry
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| Url::parse(&provider.base_url).ok())
        .and_then(|base| base.host_str().map(ToOwned::to_owned))
        .ok_or_else(|| "provider has no network validator".to_owned())?;
    if host != expected || url.port().is_some() {
        return Err("endpoint is outside the fixed provider origin".to_owned());
    }
    Ok(url)
}

/// HTTPS with a host that is not a private/loopback IP literal.
fn check_https_public(endpoint: &str) -> Result<Url, String> {
    let url = Url::parse(endpoint).map_err(|_| "invalid endpoint URL".to_owned())?;
    if url.scheme() != "https" {
        return Err("provider validation requires HTTPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "endpoint host is required".to_owned())?;
    if host.parse::<IpAddr>().map(is_private_ip).unwrap_or(false) {
        return Err("private network endpoints are not allowed".to_owned());
    }
    Ok(url)
}

pub fn validation_endpoint(provider_id: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.validation_url.as_deref())
}

/// Resolved model-list URL of a provider, always passed through `validate_endpoint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelsEndpoint {
    pub url: String,
    pub protocol: Protocol,
    /// Derived from `baseUrl` rather than the catalog's `validationUrl`.
    pub is_fallback: bool,
    /// False when the URL only validates a key (e.g. `/user`) and returns no model list.
    pub lists_models: bool,
}

/// `validationUrl` when present; otherwise `{baseUrl}/models` for OpenAI-protocol providers.
pub fn models_endpoint(provider_id: &str) -> Option<ModelsEndpoint> {
    let provider = REGISTRY
        .iter()
        .find(|provider| provider.id == provider_id)?;
    let (candidate, is_fallback) = match provider.validation_url.as_deref() {
        Some(url) => (url.to_owned(), false),
        None if provider.protocol == Protocol::Openai
            && provider.model_env_key.is_some()
            && !provider.base_url.trim().is_empty() =>
        {
            (
                format!("{}/models", provider.base_url.trim().trim_end_matches('/')),
                true,
            )
        }
        None => return None,
    };
    // A catalog `validationUrl` may live on a different host than `baseUrl`
    // (e.g. hunyuan); the catalog is compiled in, so only HTTPS/public is enforced.
    // A derived fallback is built from `baseUrl` and also gets the fixed-origin check.
    let url = if is_fallback {
        validate_endpoint(provider_id, &candidate, &REGISTRY).ok()?
    } else {
        check_https_public(&candidate).ok()?
    };
    let lists_models = url.path().trim_end_matches('/').ends_with("/models");
    Some(ModelsEndpoint {
        url: url.to_string(),
        protocol: provider.protocol,
        is_fallback,
        lists_models,
    })
}

/// Page URL for providers that paginate. Anthropic: `limit` + `after_id`
/// (platform.claude.com/docs/en/api/models-list); Gemini: `pageSize` + `pageToken`
/// (ai.google.dev/api/models; pageSize must stay the same across pages).
/// OpenAI-protocol lists are not paginated and are returned unchanged.
pub fn model_page_url(endpoint: &str, protocol: Protocol, cursor: Option<&str>) -> Option<String> {
    let (size_param, cursor_param) = match protocol {
        Protocol::Anthropic => ("limit", "after_id"),
        Protocol::Gemini => ("pageSize", "pageToken"),
        Protocol::Openai => return Some(endpoint.to_owned()),
    };
    let mut url = Url::parse(endpoint).ok()?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair(size_param, "1000");
        if let Some(cursor) = cursor {
            query.append_pair(cursor_param, cursor);
        }
    }
    Some(url.to_string())
}

/// Validation result derived from an HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub status: &'static str,
    /// The endpoint is a derived guess that does not exist; model sync is `unsupported` too.
    pub models_unsupported: bool,
}

pub fn map_validation_status(http_status: u16, is_fallback: bool) -> ValidationOutcome {
    let (status, models_unsupported) = match http_status {
        200..=299 => ("valid", false),
        401 | 403 => ("unauthorized", false),
        429 => ("rate_limited", false),
        404 | 405 if is_fallback => ("unsupported", true),
        400..=499 => ("invalid", false),
        _ => ("network_error", false),
    };
    ValidationOutcome {
        status,
        models_unsupported,
    }
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => {
            value.is_private()
                || value.is_loopback()
                || value.is_link_local()
                || value.is_unspecified()
        }
        IpAddr::V6(value) => {
            value.is_loopback()
                || value.is_unique_local()
                || value.is_unicast_link_local()
                || value.is_unspecified()
        }
    }
}
