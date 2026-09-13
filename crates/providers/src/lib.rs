//! Deterministic provider registry and local format detection.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDefinition { pub id: String, pub name: String, pub env_keys: Vec<String>, pub prefixes: Vec<String> }

pub fn default_registry() -> Vec<ProviderDefinition> {
    vec![
        provider("openai", "OpenAI", &["OPENAI_API_KEY"], &["sk-"]),
        provider("anthropic", "Anthropic", &["ANTHROPIC_API_KEY"], &["sk-ant-"]),
        provider("gemini", "Google Gemini", &["GEMINI_API_KEY"], &["AIza"]),
        provider("deepseek", "DeepSeek", &["DEEPSEEK_API_KEY"], &["sk-deep-"]),
        provider("xai", "xAI", &["XAI_API_KEY"], &["xai-"]),
        provider("openrouter", "OpenRouter", &["OPENROUTER_API_KEY"], &["sk-or-"]),
        provider("github", "GitHub", &["GITHUB_TOKEN"], &["ghp_", "github_pat_"]),
        provider("supabase", "Supabase", &["SUPABASE_URL", "SUPABASE_ANON_KEY"], &["sbp_"]),
        provider("cloudflare", "Cloudflare", &["CLOUDFLARE_API_TOKEN"], &["cf_"]),
        provider("generic", "Generic API Key", &["API_KEY"], &[]),
    ]
}

pub fn detect_provider(value: &str, registry: &[ProviderDefinition]) -> Option<ProviderDefinition> {
    registry.iter().filter_map(|provider| provider.prefixes.iter().filter(|prefix| value.starts_with(prefix.as_str())).max_by_key(|prefix| prefix.len()).map(|prefix| (prefix.len(), provider))).max_by_key(|(length, _)| *length).map(|(_, provider)| provider.clone())
}

fn provider(id: &str, name: &str, env_keys: &[&str], prefixes: &[&str]) -> ProviderDefinition { ProviderDefinition { id: id.into(), name: name.into(), env_keys: env_keys.iter().map(|value| (*value).into()).collect(), prefixes: prefixes.iter().map(|value| (*value).into()).collect() } }
