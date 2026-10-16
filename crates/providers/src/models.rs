//! Pure model-list parsing and capability inference. No network access here.

use crate::{Protocol, ProviderDefinition};
use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use serde_json::Value;
use std::{fmt, sync::LazyLock};

/// Fixed capability vocabulary shared by Rust, TS and skills.
pub const CAPABILITY_VOCABULARY: [&str; 10] = [
    "chat",
    "code",
    "reasoning",
    "vision",
    "image_gen",
    "video_gen",
    "tts",
    "asr",
    "embedding",
    "rerank",
];

const MAX_MODELS: usize = 2000;
const MAX_ID_CHARS: usize = 200;
const MAX_NAME_CHARS: usize = 200;
const MAX_TOKEN_CHARS: usize = 64;
const MAX_LIST_ITEMS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscoveredModel {
    pub id: String,
    pub display_name: Option<String>,
    pub input: Vec<String>,
    pub output: Vec<String>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelPage {
    pub models: Vec<DiscoveredModel>,
    /// Anthropic `last_id` (when `has_more`) or Gemini `nextPageToken`.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model list parse error: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilitySource {
    Provider,
    Catalog,
    Heuristic,
    Unknown,
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Lower-cased, truncated, bounded list of strings from a JSON array.
fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|text| truncate(text.trim(), MAX_TOKEN_CHARS).to_lowercase())
                .filter(|text| !text.is_empty())
                .take(MAX_LIST_ITEMS)
                .collect()
        })
        .unwrap_or_default()
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_owned());
    }
}

pub fn parse_model_list(protocol: Protocol, body: &str) -> Result<ModelPage, ParseError> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| ParseError("response is not JSON".to_owned()))?;
    let key = match protocol {
        Protocol::Gemini => "models",
        Protocol::Openai | Protocol::Anthropic => "data",
    };
    let items = root
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError(format!("missing `{key}` array")))?;
    let models = items
        .iter()
        .filter_map(|item| match protocol {
            Protocol::Openai => parse_openai(item),
            Protocol::Anthropic => parse_anthropic(item),
            Protocol::Gemini => parse_gemini(item),
        })
        .take(MAX_MODELS)
        .collect();
    let next_cursor = match protocol {
        Protocol::Openai => None,
        Protocol::Anthropic => {
            if root.get("has_more").and_then(Value::as_bool) == Some(true) {
                str_field(&root, "last_id").map(|id| truncate(&id, 512))
            } else {
                None
            }
        }
        Protocol::Gemini => str_field(&root, "nextPageToken").map(|token| truncate(&token, 2048)),
    };
    Ok(ModelPage {
        models,
        next_cursor,
    })
}

fn parse_openai(item: &Value) -> Option<DiscoveredModel> {
    let id = str_field(item, "id")?;
    let architecture = item.get("architecture");
    let mut hints = Vec::new();
    for parameter in string_list(item.get("supported_parameters")) {
        if parameter == "reasoning" || parameter == "tools" {
            push_unique(&mut hints, &parameter);
        }
    }
    // new-api style gateways
    for endpoint in string_list(item.get("supported_endpoint_types")) {
        push_unique(&mut hints, &endpoint);
    }
    Some(DiscoveredModel {
        id: truncate(&id, MAX_ID_CHARS),
        display_name: str_field(item, "name").map(|name| truncate(&name, MAX_NAME_CHARS)),
        input: string_list(architecture.and_then(|a| a.get("input_modalities"))),
        output: string_list(architecture.and_then(|a| a.get("output_modalities"))),
        hints,
    })
}

fn parse_anthropic(item: &Value) -> Option<DiscoveredModel> {
    let id = str_field(item, "id")?;
    let capabilities = item.get("capabilities");
    let supported = |name: &str| {
        capabilities
            .and_then(|c| c.get(name))
            .and_then(|c| c.get("supported"))
            .and_then(Value::as_bool)
            == Some(true)
    };
    // Claude models list text output; image input is reported explicitly.
    let mut input = vec!["text".to_owned()];
    if supported("image_input") {
        input.push("image".to_owned());
    }
    let hints = if supported("thinking") {
        vec!["reasoning".to_owned()]
    } else {
        Vec::new()
    };
    Some(DiscoveredModel {
        id: truncate(&id, MAX_ID_CHARS),
        display_name: str_field(item, "display_name").map(|name| truncate(&name, MAX_NAME_CHARS)),
        input,
        output: vec!["text".to_owned()],
        hints,
    })
}

fn parse_gemini(item: &Value) -> Option<DiscoveredModel> {
    let name = str_field(item, "name")?;
    let id = name.strip_prefix("models/").unwrap_or(&name);
    if id.is_empty() {
        return None;
    }
    let mut hints: Vec<String> = item
        .get("supportedGenerationMethods")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|text| truncate(text.trim(), MAX_TOKEN_CHARS))
                .filter(|text| !text.is_empty())
                .take(MAX_LIST_ITEMS)
                .collect()
        })
        .unwrap_or_default();
    if item.get("thinking").and_then(Value::as_bool) == Some(true) {
        push_unique(&mut hints, "reasoning");
    }
    Some(DiscoveredModel {
        id: truncate(id, MAX_ID_CHARS),
        display_name: str_field(item, "displayName").map(|name| truncate(&name, MAX_NAME_CHARS)),
        input: Vec::new(),
        output: Vec::new(),
        hints,
    })
}

#[derive(Debug, Clone)]
pub struct CapabilityRule {
    pub pattern: Regex,
    pub capabilities: Vec<String>,
}

#[derive(Deserialize)]
struct RawRule {
    pattern: String,
    capabilities: Vec<String>,
}

/// Parses `capability-rules.json`: ordered `{ pattern, capabilities }` entries,
/// patterns are case-insensitive regexes.
pub fn load_rules(json: &str) -> Result<Vec<CapabilityRule>, String> {
    let raw: Vec<RawRule> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    raw.into_iter()
        .map(|rule| {
            if let Some(unknown) = rule
                .capabilities
                .iter()
                .find(|cap| !CAPABILITY_VOCABULARY.contains(&cap.as_str()))
            {
                return Err(format!("unknown capability `{unknown}`"));
            }
            let pattern = RegexBuilder::new(&rule.pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| format!("{}: {error}", rule.pattern))?;
            Ok(CapabilityRule {
                pattern,
                capabilities: rule.capabilities,
            })
        })
        .collect()
}

static RULES: LazyLock<Vec<CapabilityRule>> = LazyLock::new(|| {
    load_rules(include_str!(
        "../../../packages/provider-definitions/capability-rules.json"
    ))
    .expect("packages/provider-definitions/capability-rules.json is valid")
});

pub fn default_rules() -> &'static [CapabilityRule] {
    &RULES
}

/// Capabilities implied by provider-reported data (modalities and hints).
fn provider_capabilities(model: &DiscoveredModel) -> Vec<String> {
    let has = |list: &[String], value: &str| list.iter().any(|item| item == value);
    let mut caps: Vec<String> = Vec::new();
    let mut media = false;
    let mut text_chat = has(&model.output, "text");
    if has(&model.input, "image") {
        push_unique(&mut caps, "vision");
    }
    for (modality, capability) in [
        ("image", "image_gen"),
        ("audio", "tts"),
        ("video", "video_gen"),
    ] {
        if has(&model.output, modality) {
            push_unique(&mut caps, capability);
            media = true;
        }
    }
    for hint in &model.hints {
        let capability = match hint.as_str() {
            "reasoning" => "reasoning",
            "embedContent" | "embedText" | "batchEmbedContents" | "embeddings" => "embedding",
            "jina-rerank" => "rerank",
            "image-generation" => "image_gen",
            "openai-video" => "video_gen",
            "generateContent" | "openai" | "openai-response" | "anthropic" | "gemini" => {
                text_chat = true;
                continue;
            }
            _ => continue,
        };
        if !matches!(capability, "reasoning") {
            media = true;
        }
        push_unique(&mut caps, capability);
    }
    if text_chat && !media {
        push_unique(&mut caps, "chat");
    }
    caps
}

fn canonical(mut caps: Vec<String>) -> Vec<String> {
    caps.retain(|cap| CAPABILITY_VOCABULARY.contains(&cap.as_str()));
    caps.sort_by_key(|cap| CAPABILITY_VOCABULARY.iter().position(|v| v == cap));
    caps.dedup();
    caps
}

/// Priority (override handling lives with storage): provider > catalog > heuristic > unknown.
/// Provider data is merged with heuristic matches.
pub fn infer_capabilities(
    provider_id: &str,
    model: &DiscoveredModel,
    registry: &[ProviderDefinition],
    rules: &[CapabilityRule],
) -> (Vec<String>, CapabilitySource) {
    let heuristic = || {
        rules
            .iter()
            .filter(|rule| rule.pattern.is_match(&model.id))
            .flat_map(|rule| rule.capabilities.iter().cloned())
            .collect::<Vec<_>>()
    };
    let mut provider = provider_capabilities(model);
    if !provider.is_empty() {
        provider.extend(heuristic());
        return (canonical(provider), CapabilitySource::Provider);
    }
    let catalog = registry
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.models.iter().find(|entry| entry.id == model.id))
        .map(|entry| canonical(entry.capabilities.clone()))
        .filter(|caps| !caps.is_empty());
    if let Some(caps) = catalog {
        return (caps, CapabilitySource::Catalog);
    }
    let caps = canonical(heuristic());
    if caps.is_empty() {
        (caps, CapabilitySource::Unknown)
    } else {
        (caps, CapabilitySource::Heuristic)
    }
}
