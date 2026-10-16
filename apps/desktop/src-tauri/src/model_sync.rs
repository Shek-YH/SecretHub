//! Non-blocking provider request used for key validation and model-list sync.
//! Never logs or returns the key value; the caller owns and drops it.

use reqwest::{redirect::Policy, Client, RequestBuilder};
use secrethub_providers::{
    default_registry, default_rules, infer_capabilities, model_page_url, parse_model_list,
    DiscoveredModel, ModelsEndpoint, Protocol,
};
use secrethub_storage::ProviderModelRow;
use std::{collections::HashSet, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGES: usize = 5;
const MAX_MODELS: usize = 2000;

pub struct Fetched {
    /// HTTP status of the first page; `None` when the request itself failed.
    pub http_status: Option<u16>,
    /// Parsed models; `None` when not requested, not 2xx, or reading/parsing failed.
    pub models: Option<Vec<DiscoveredModel>>,
}

/// Auth header scheme comes from the provider's protocol, not its id.
fn with_auth(request: RequestBuilder, protocol: Protocol, value: &str) -> RequestBuilder {
    match protocol {
        Protocol::Anthropic => request
            .header("x-api-key", value)
            .header("anthropic-version", "2023-06-01"),
        Protocol::Gemini => request.header("x-goog-api-key", value),
        Protocol::Openai => request.bearer_auth(value),
    }
}

/// Reads at most `MAX_BODY_BYTES`; `None` when the cap is exceeded or the read fails.
async fn read_capped(mut response: reqwest::Response) -> Option<String> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).ok()
}

/// One validation request plus, when `want_models`, the paginated model list.
pub async fn fetch(endpoint: &ModelsEndpoint, value: &str, want_models: bool) -> Fetched {
    let Ok(client) = Client::builder()
        .redirect(Policy::none())
        .timeout(TIMEOUT)
        .build()
    else {
        return Fetched {
            http_status: None,
            models: None,
        };
    };
    let max_pages = match endpoint.protocol {
        Protocol::Openai => 1,
        Protocol::Anthropic | Protocol::Gemini => MAX_PAGES,
    };
    let mut http_status = None;
    let mut models: Vec<DiscoveredModel> = Vec::new();
    let mut cursor: Option<String> = None;
    for page in 0..max_pages {
        let failed = |http_status| Fetched {
            http_status,
            models: None,
        };
        let Some(url) = model_page_url(&endpoint.url, endpoint.protocol, cursor.as_deref()) else {
            return failed(http_status);
        };
        let Ok(response) = with_auth(client.get(url), endpoint.protocol, value)
            .send()
            .await
        else {
            return failed(http_status);
        };
        let status = response.status();
        if page == 0 {
            http_status = Some(status.as_u16());
            if !status.is_success() || !want_models {
                return failed(http_status);
            }
        } else if !status.is_success() {
            return failed(http_status);
        }
        let Some(body) = read_capped(response).await else {
            return failed(http_status);
        };
        let Ok(parsed) = parse_model_list(endpoint.protocol, &body) else {
            return failed(http_status);
        };
        models.extend(parsed.models);
        // Stop on a missing or repeated cursor so a misbehaving server cannot loop us.
        match parsed.next_cursor {
            Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
            _ => break,
        }
        if models.len() >= MAX_MODELS {
            break;
        }
    }
    models.truncate(MAX_MODELS);
    Fetched {
        http_status,
        models: Some(models),
    }
}

/// Infers capabilities and builds storage rows (deduplicated by model id).
pub fn build_rows(provider_id: &str, models: &[DiscoveredModel]) -> Vec<ProviderModelRow> {
    let registry = default_registry();
    let rules = default_rules();
    let mut seen = HashSet::new();
    models
        .iter()
        .filter(|model| seen.insert(model.id.clone()))
        .map(|model| {
            let (capabilities, source) = infer_capabilities(provider_id, model, &registry, rules);
            ProviderModelRow {
                model_id: model.id.clone(),
                display_name: model.display_name.clone().unwrap_or_default(),
                input: model.input.clone(),
                output: model.output.clone(),
                capabilities,
                capability_source: source.as_str().to_owned(),
                synced_at: 0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_deduplicated_rows_with_inferred_capabilities() {
        let model = |id: &str| DiscoveredModel {
            id: id.into(),
            ..Default::default()
        };
        let rows = build_rows(
            "openrouter",
            &[
                model("qwen3-coder"),
                model("qwen3-coder"),
                model("zzz-unknown"),
            ],
        );
        assert_eq!(rows.len(), 2);
        assert!(rows[0].capabilities.iter().any(|cap| cap == "code"));
        assert_eq!(rows[1].capability_source, "unknown");
        assert!(rows[1].capabilities.is_empty());
    }
}
