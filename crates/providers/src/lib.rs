//! Deterministic provider registry and local format detection.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDefinition {
    pub id: String,
    pub name: String,
    pub env_keys: Vec<String>,
    pub prefixes: Vec<String>,
}

pub fn default_registry() -> Vec<ProviderDefinition> {
    vec![
        provider("openai", "OpenAI", &["OPENAI_API_KEY"], &["sk-"]),
        provider(
            "anthropic",
            "Anthropic",
            &["ANTHROPIC_API_KEY"],
            &["sk-ant-"],
        ),
        provider("gemini", "Google Gemini", &["GEMINI_API_KEY"], &["AIza"]),
        provider("deepseek", "DeepSeek", &["DEEPSEEK_API_KEY"], &["sk-deep-"]),
        provider("xai", "xAI", &["XAI_API_KEY"], &["xai-"]),
        provider(
            "openrouter",
            "OpenRouter",
            &["OPENROUTER_API_KEY"],
            &["sk-or-"],
        ),
        provider(
            "github",
            "GitHub",
            &["GITHUB_TOKEN"],
            &["ghp_", "github_pat_"],
        ),
        provider(
            "supabase",
            "Supabase",
            &["SUPABASE_URL", "SUPABASE_ANON_KEY"],
            &["sbp_"],
        ),
        provider(
            "cloudflare",
            "Cloudflare",
            &["CLOUDFLARE_API_TOKEN"],
            &["cf_"],
        ),
        provider("generic", "Generic API Key", &["API_KEY"], &[]),
    ]
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
    _registry: &[ProviderDefinition],
) -> Result<Url, String> {
    let url = Url::parse(endpoint).map_err(|_| "invalid endpoint URL".to_owned())?;
    if url.scheme() != "https" {
        return Err("provider validation requires HTTPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "endpoint host is required".to_owned())?;
    let expected = match provider_id {
        "openai" => "api.openai.com",
        "anthropic" => "api.anthropic.com",
        "gemini" => "generativelanguage.googleapis.com",
        "deepseek" => "api.deepseek.com",
        "xai" => "api.x.ai",
        "openrouter" => "openrouter.ai",
        "github" => "api.github.com",
        "supabase" => "api.supabase.com",
        "cloudflare" => "api.cloudflare.com",
        _ => return Err("provider has no network validator".to_owned()),
    };
    if host != expected || url.port().is_some() {
        return Err("endpoint is outside the fixed provider origin".to_owned());
    }
    if host.parse::<IpAddr>().map(is_private_ip).unwrap_or(false) {
        return Err("private network endpoints are not allowed".to_owned());
    }
    Ok(url)
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

fn provider(id: &str, name: &str, env_keys: &[&str], prefixes: &[&str]) -> ProviderDefinition {
    ProviderDefinition {
        id: id.into(),
        name: name.into(),
        env_keys: env_keys.iter().map(|value| (*value).into()).collect(),
        prefixes: prefixes.iter().map(|value| (*value).into()).collect(),
    }
}
