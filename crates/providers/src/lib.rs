//! Deterministic provider registry and local format detection.

use serde::{Deserialize, Serialize};
use std::{net::IpAddr, sync::LazyLock};
use url::Url;

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
    let url = Url::parse(endpoint).map_err(|_| "invalid endpoint URL".to_owned())?;
    if url.scheme() != "https" {
        return Err("provider validation requires HTTPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "endpoint host is required".to_owned())?;
    let expected = registry
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| Url::parse(&provider.base_url).ok())
        .and_then(|base| base.host_str().map(ToOwned::to_owned))
        .ok_or_else(|| "provider has no network validator".to_owned())?;
    if host != expected || url.port().is_some() {
        return Err("endpoint is outside the fixed provider origin".to_owned());
    }
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
