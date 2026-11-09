//! AI capability suggestions (design §7): `caps-*.json` files queued by the MCP server.
//! They hold model ids and capability tags only; nothing here touches secret values.
//! Accepted suggestions become `ai` overrides, everything else is discarded.

use crate::plan::CAPABILITIES;
use secrethub_core::VaultService;
use secrethub_providers::default_registry;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const KIND: &str = "suggest_model_capabilities";
const MAX_SUGGESTIONS: usize = 200;
const MAX_MODEL_ID_LEN: usize = 200;
const MAX_EVIDENCE_LEN: usize = 500;
const MAX_PROVIDER_LEN: usize = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct RawSuggestion {
    #[serde(alias = "modelId")]
    pub model_id: String,
    pub capabilities: Vec<String>,
    #[serde(default, alias = "evidenceUrl")]
    pub evidence_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PendingSuggestions {
    #[serde(default)]
    pub version: u32,
    #[serde(alias = "requestId")]
    pub request_id: String,
    pub kind: String,
    #[serde(alias = "providerId")]
    pub provider_id: String,
    pub suggestions: Vec<RawSuggestion>,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

/// Stored capabilities of a model, as the secret detail list would show them.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CurrentCapabilities {
    pub capabilities: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionView {
    pub model_id: String,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<CurrentCapabilities>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionBatch {
    pub request_id: String,
    pub provider_id: String,
    pub created_at: String,
    pub suggestions: Vec<SuggestionView>,
}

pub fn valid_request_id(id: &str) -> bool {
    id.starts_with("caps-")
        && id.len() <= 128
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

impl PendingSuggestions {
    /// Checks the file against the same rules the MCP tool enforces and normalizes it
    /// (trimmed model ids, de-duplicated capabilities). Messages never echo file content.
    pub fn validated(mut self, expected_id: &str) -> Result<Self, String> {
        let invalid = |reason: &str| Err(format!("invalid capability suggestions: {reason}"));
        if self.request_id != expected_id || !valid_request_id(&self.request_id) {
            return invalid("request id");
        }
        if self.kind != KIND || self.version > 1 {
            return invalid("kind or version");
        }
        let provider = self.provider_id.trim();
        if provider.is_empty()
            || provider.chars().count() > MAX_PROVIDER_LEN
            || has_control(provider)
            || !default_registry().iter().any(|item| item.id == provider)
        {
            return invalid("provider");
        }
        self.provider_id = provider.to_owned();
        if self.suggestions.is_empty() || self.suggestions.len() > MAX_SUGGESTIONS {
            return invalid("suggestion count");
        }
        let mut seen = HashSet::new();
        for suggestion in &mut self.suggestions {
            let model_id = suggestion.model_id.trim();
            if model_id.is_empty()
                || model_id.chars().count() > MAX_MODEL_ID_LEN
                || has_control(model_id)
                || !seen.insert(model_id.to_owned())
            {
                return invalid("model id");
            }
            suggestion.model_id = model_id.to_owned();
            let mut capabilities: Vec<String> = Vec::new();
            for capability in &suggestion.capabilities {
                if !CAPABILITIES.contains(&capability.as_str()) {
                    return invalid("capability");
                }
                if !capabilities.contains(capability) {
                    capabilities.push(capability.clone());
                }
            }
            if capabilities.is_empty() {
                return invalid("capability");
            }
            suggestion.capabilities = capabilities;
            if let Some(url) = &suggestion.evidence_url {
                let https = url::Url::parse(url)
                    .map(|parsed| parsed.scheme() == "https" && parsed.host_str().is_some())
                    .unwrap_or(false);
                if url.chars().count() > MAX_EVIDENCE_LEN || !https {
                    return invalid("evidence url");
                }
            }
        }
        Ok(self)
    }
}

fn read_file(path: &Path, request_id: &str) -> Result<PendingSuggestions, String> {
    let contents =
        fs::read_to_string(path).map_err(|_| "capability suggestions not found".to_owned())?;
    serde_json::from_str::<PendingSuggestions>(&contents)
        .map_err(|_| "invalid capability suggestions: unreadable file".to_owned())?
        .validated(request_id)
}

pub fn pending_path(directory: &Path, request_id: &str) -> Result<PathBuf, String> {
    if !valid_request_id(request_id) {
        return Err("invalid capability suggestion id".to_owned());
    }
    Ok(directory.join(format!("{request_id}.json")))
}

/// Valid `caps-*.json` files only, oldest first; `plan-*` and malformed files are ignored.
pub fn read_pending(directory: &Path) -> Vec<PendingSuggestions> {
    let Ok(read) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut batches = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_owned();
            if !valid_request_id(&stem) {
                return None;
            }
            read_file(&path, &stem).ok()
        })
        .collect::<Vec<_>>();
    batches.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    batches
}

/// Adds the currently stored capabilities where a secret of the provider has the model synced.
pub fn enrich(batch: &PendingSuggestions, vault: &VaultService) -> SuggestionBatch {
    let mut current: std::collections::HashMap<String, CurrentCapabilities> =
        std::collections::HashMap::new();
    // A locked vault just means no "current" column; the suggestions themselves are metadata.
    if let Ok(metadata) = vault.list_metadata() {
        for item in metadata
            .iter()
            .filter(|item| item.provider_id == batch.provider_id)
        {
            for row in vault.list_provider_models(&item.id).unwrap_or_default() {
                current
                    .entry(row.model_id)
                    .or_insert_with(|| CurrentCapabilities {
                        capabilities: row.capabilities,
                        source: row.capability_source,
                    });
            }
        }
    }
    SuggestionBatch {
        request_id: batch.request_id.clone(),
        provider_id: batch.provider_id.clone(),
        created_at: batch.created_at.clone(),
        suggestions: batch
            .suggestions
            .iter()
            .map(|suggestion| SuggestionView {
                model_id: suggestion.model_id.clone(),
                capabilities: suggestion.capabilities.clone(),
                evidence_url: suggestion.evidence_url.clone(),
                current: current.get(&suggestion.model_id).cloned(),
            })
            .collect(),
    }
}

pub fn enriched_pending(directory: &Path, vault: Option<&VaultService>) -> Vec<SuggestionBatch> {
    read_pending(directory)
        .iter()
        .map(|batch| match vault {
            Some(vault) => enrich(batch, vault),
            None => SuggestionBatch {
                request_id: batch.request_id.clone(),
                provider_id: batch.provider_id.clone(),
                created_at: batch.created_at.clone(),
                suggestions: batch
                    .suggestions
                    .iter()
                    .map(|suggestion| SuggestionView {
                        model_id: suggestion.model_id.clone(),
                        capabilities: suggestion.capabilities.clone(),
                        evidence_url: suggestion.evidence_url.clone(),
                        current: None,
                    })
                    .collect(),
            },
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Resolution {
    pub accepted: usize,
    pub rejected: usize,
}

/// Writes the accepted suggestions as `ai` overrides, records a value-free audit action and
/// deletes the file. An empty `accepted` list rejects everything. Any error before the
/// overrides are written leaves the file pending (a locked vault can be retried).
pub fn resolve(
    directory: &Path,
    request_id: &str,
    accepted: &[String],
    vault: &VaultService,
) -> Result<Resolution, String> {
    let path = pending_path(directory, request_id)?;
    let batch = match read_file(&path, request_id) {
        Ok(batch) => batch,
        Err(message) => {
            // A malformed file can never be accepted; drop it so it cannot clog the queue.
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
            return Err(message);
        }
    };
    let accepted = accepted.iter().map(String::as_str).collect::<HashSet<_>>();
    if accepted
        .iter()
        .any(|id| !batch.suggestions.iter().any(|item| item.model_id == *id))
    {
        return Err("selection contains an unknown model id".to_owned());
    }
    let mut written = 0;
    for suggestion in batch
        .suggestions
        .iter()
        .filter(|item| accepted.contains(item.model_id.as_str()))
    {
        vault
            .set_capability_override(
                &batch.provider_id,
                &suggestion.model_id,
                &suggestion.capabilities,
                "ai",
            )
            .map_err(|error| error.to_string())?;
        written += 1;
    }
    let rejected = batch.suggestions.len() - written;
    // Overrides are stored; an audit failure must not undo or hide that.
    let _ = vault.record_action(
        "capability_suggestions_resolved",
        None,
        &format!("{{\"source\":\"desktop\",\"accepted\":{written},\"rejected\":{rejected}}}"),
    );
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    Ok(Resolution {
        accepted: written,
        rejected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_core::NewSecretInput;
    use secrethub_storage::{Database, ProviderModelRow};

    fn file(id: &str, body: &str) -> String {
        format!(
            r#"{{"version":1,"requestId":"{id}","kind":"suggest_model_capabilities","providerId":"openrouter","suggestions":{body},"createdAt":"2026-10-16T00:00:00Z"}}"#
        )
    }

    fn parse(id: &str, body: &str) -> Result<PendingSuggestions, String> {
        serde_json::from_str::<PendingSuggestions>(&file(id, body))
            .unwrap()
            .validated(id)
    }

    #[test]
    fn request_ids_need_the_caps_prefix_and_a_safe_charset() {
        assert!(valid_request_id("caps-1760000000000-abc123"));
        for bad in [
            "",
            "caps",
            "plan-1",
            "caps-../a",
            "caps-a/b",
            "caps-a\\b",
            "caps-a.json",
            &format!("caps-{}", "a".repeat(200)),
        ] {
            assert!(!valid_request_id(bad), "{bad}");
        }
        assert!(pending_path(Path::new("."), "plan-1").is_err());
    }

    #[test]
    fn valid_files_are_normalized() {
        let batch = parse(
            "caps-1",
            r#"[{"modelId":" m1 ","capabilities":["chat","vision","chat"],"evidenceUrl":"https://docs.example/m1"},{"model_id":"m2","capabilities":["tts"]}]"#,
        )
        .unwrap();
        assert_eq!(batch.suggestions[0].model_id, "m1");
        assert_eq!(batch.suggestions[0].capabilities, ["chat", "vision"]);
        assert_eq!(batch.suggestions[1].evidence_url, None);
    }

    #[test]
    fn invalid_files_are_rejected_without_echoing_content() {
        let long = "a".repeat(201);
        let cases = vec![
            "[]".to_owned(),
            r#"[{"modelId":"m","capabilities":[]}]"#.to_owned(),
            r#"[{"modelId":"m","capabilities":["telepathy"]}]"#.to_owned(),
            r#"[{"modelId":" ","capabilities":["chat"]}]"#.to_owned(),
            r#"[{"modelId":"a\nb","capabilities":["chat"]}]"#.to_owned(),
            format!(r#"[{{"modelId":"{long}","capabilities":["chat"]}}]"#),
            r#"[{"modelId":"m","capabilities":["chat"]},{"modelId":"m","capabilities":["code"]}]"#
                .to_owned(),
            r#"[{"modelId":"m","capabilities":["chat"],"evidenceUrl":"http://x.example"}]"#
                .to_owned(),
            r#"[{"modelId":"m","capabilities":["chat"],"evidenceUrl":"javascript:alert(1)"}]"#
                .to_owned(),
        ];
        for body in cases {
            let error = parse("caps-1", &body).err().unwrap_or_default();
            assert!(
                error.starts_with("invalid capability suggestions"),
                "{body}"
            );
            assert!(!error.contains("telepathy") && !error.contains("javascript"));
        }
        let many = format!(
            "[{}]",
            (0..201)
                .map(|index| format!(r#"{{"modelId":"m{index}","capabilities":["chat"]}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(parse("caps-1", &many).is_err());
        let wrong_kind = file("caps-1", r#"[{"modelId":"m","capabilities":["chat"]}]"#)
            .replace("suggest_model_capabilities", "other");
        assert!(serde_json::from_str::<PendingSuggestions>(&wrong_kind)
            .unwrap()
            .validated("caps-1")
            .is_err());
        let unknown_provider = file("caps-1", r#"[{"modelId":"m","capabilities":["chat"]}]"#)
            .replace("openrouter", "no-such-provider");
        assert!(
            serde_json::from_str::<PendingSuggestions>(&unknown_provider)
                .unwrap()
                .validated("caps-1")
                .is_err()
        );
        // The id inside the file must match the file name.
        assert!(serde_json::from_str::<PendingSuggestions>(&file(
            "caps-1",
            r#"[{"modelId":"m","capabilities":["chat"]}]"#
        ))
        .unwrap()
        .validated("caps-2")
        .is_err());
    }

    #[test]
    fn readers_ignore_plan_files_and_garbage() {
        let directory = tempfile::tempdir().unwrap();
        let body = r#"[{"modelId":"m","capabilities":["chat"]}]"#;
        fs::write(directory.path().join("caps-1.json"), file("caps-1", body)).unwrap();
        fs::write(directory.path().join("caps-2.json"), "not json").unwrap();
        fs::write(directory.path().join("plan-3.json"), file("plan-3", body)).unwrap();
        fs::write(
            directory.path().join("caps-4.json.tmp"),
            file("caps-4", body),
        )
        .unwrap();
        let ids = read_pending(directory.path())
            .into_iter()
            .map(|batch| batch.request_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, ["caps-1"]);
    }

    fn vault_with_synced_model(directory: &Path) -> (VaultService, String) {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        let id = vault
            .create_secret(NewSecretInput {
                name: "Test key".to_owned(),
                provider_id: "openrouter".to_owned(),
                env_key: "OPENROUTER_API_KEY".to_owned(),
                description: String::new(),
                tags: Vec::new(),
                value: "fake-test-value-do-not-leak".to_owned(),
            })
            .unwrap();
        vault
            .replace_provider_models(
                &id,
                &[ProviderModelRow {
                    model_id: "known".to_owned(),
                    display_name: String::new(),
                    input: Vec::new(),
                    output: Vec::new(),
                    capabilities: vec!["chat".to_owned()],
                    capability_source: "heuristic".to_owned(),
                    synced_at: 1,
                }],
                1,
                "ok",
            )
            .unwrap();
        (vault, id)
    }

    #[test]
    fn accept_writes_ai_overrides_and_reject_discards() {
        let data = tempfile::tempdir().unwrap();
        let pending = tempfile::tempdir().unwrap();
        let (vault, secret_id) = vault_with_synced_model(data.path());
        let body = r#"[{"modelId":"known","capabilities":["chat","vision"],"evidenceUrl":"https://docs.example/known"},{"modelId":"other","capabilities":["tts"]}]"#;
        let path = pending.path().join("caps-1.json");
        fs::write(&path, file("caps-1", body)).unwrap();

        let listed = enriched_pending(pending.path(), Some(&vault));
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].suggestions[0].current,
            Some(CurrentCapabilities {
                capabilities: vec!["chat".to_owned()],
                source: "heuristic".to_owned()
            })
        );
        assert_eq!(listed[0].suggestions[1].current, None);

        assert!(resolve(pending.path(), "caps-1", &["nope".to_owned()], &vault).is_err());
        assert!(path.exists());
        let outcome = resolve(pending.path(), "caps-1", &["known".to_owned()], &vault).unwrap();
        assert_eq!(
            outcome,
            Resolution {
                accepted: 1,
                rejected: 1
            }
        );
        assert!(!path.exists());
        let models = vault.list_provider_models(&secret_id).unwrap();
        assert_eq!(models[0].capabilities, ["chat", "vision"]);
        assert_eq!(models[0].capability_source, "ai");
        let audit = vault.list_audit().unwrap();
        assert!(audit
            .iter()
            .any(|event| event.operation == "capability_suggestions_resolved"));

        // Reject all: nothing is written, file is gone.
        fs::write(&path, file("caps-1", body)).unwrap();
        let outcome = resolve(pending.path(), "caps-1", &[], &vault).unwrap();
        assert_eq!(
            outcome,
            Resolution {
                accepted: 0,
                rejected: 2
            }
        );
        assert!(!path.exists());
        assert_eq!(
            vault.list_provider_models(&secret_id).unwrap()[0].capabilities,
            ["chat", "vision"]
        );
    }

    #[test]
    fn locked_vault_keeps_the_file_and_bad_files_are_dropped() {
        let data = tempfile::tempdir().unwrap();
        let pending = tempfile::tempdir().unwrap();
        let (mut vault, _) = vault_with_synced_model(data.path());
        let path = pending.path().join("caps-1.json");
        fs::write(
            &path,
            file("caps-1", r#"[{"modelId":"known","capabilities":["code"]}]"#),
        )
        .unwrap();
        vault.lock();
        assert!(resolve(pending.path(), "caps-1", &["known".to_owned()], &vault).is_err());
        assert!(path.exists());
        // Locked listing still works, just without the "current" column.
        assert_eq!(enriched_pending(pending.path(), Some(&vault)).len(), 1);

        let bad = pending.path().join("caps-2.json");
        fs::write(&bad, "{").unwrap();
        assert!(resolve(pending.path(), "caps-2", &[], &vault).is_err());
        assert!(!bad.exists());
        assert!(resolve(pending.path(), "plan-1", &[], &vault).is_err());
    }
}
