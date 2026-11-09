//! Mixed-selection export (design section 2): API keys, developer secrets, config groups (with
//! variable subsets) and dev-use logins go into one project's `.env` after a preview that
//! shows name, source, state and (for plain values only) the value. Secret values are decrypted
//! only inside `export_apply_v2`, right before writing.

use super::commands::{entries_for, touch_activity, vault_from_state};
use super::plan::{self, WriteOptions, MASK};
use super::AppState;
use secrethub_core::VaultService;
use secrethub_exporter::{detect_conflicts, validate_export_directory};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSelection {
    pub secret_id: String,
    /// Config groups only; `None` means every variable.
    #[serde(default)]
    pub variables: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequestV2 {
    pub directory: String,
    pub selections: Vec<ExportSelection>,
    /// Variables the user chose not to overwrite (or to skip) in the target `.env`.
    #[serde(default)]
    pub skip_keys: Vec<String>,
}

/// One variable that would be written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    pub key: String,
    pub secret_id: String,
    /// Name of the item the variable comes from (display only).
    pub source_name: String,
    pub secret: bool,
    /// Masked for secret values, the value itself for plain ones.
    pub display: String,
    /// `new` | `overwrite` | `conflict` (the same name comes from two selected items).
    pub status: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreviewV2 {
    pub rows: Vec<ExportRow>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcomeV2 {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// A row with its real value; never leaves this module.
struct Resolved {
    row: ExportRow,
    value: String,
}

fn resolve_rows(
    vault: &VaultService,
    selections: &[ExportSelection],
) -> Result<Vec<Resolved>, String> {
    if selections.is_empty() {
        return Err("select at least one secret".to_owned());
    }
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    let mut seen_ids = HashSet::new();
    let mut resolved = Vec::new();
    for selection in selections {
        if !seen_ids.insert(selection.secret_id.as_str()) {
            return Err("the same item is selected twice".to_owned());
        }
        let item = metadata
            .iter()
            .find(|item| item.id == selection.secret_id)
            .ok_or_else(|| "secret not found".to_owned())?;
        if !item.ai_visible() {
            return Err(super::commands::UNAVAILABLE.to_owned());
        }
        if selection.variables.is_some() && item.value_type != "env_group" {
            return Err("variables are only allowed for config groups".to_owned());
        }
        let secrets: Vec<(String, String, bool)> = if item.value_type == "env_group" {
            vault
                .group_values(&item.id, selection.variables.as_deref())
                .map_err(|error| error.to_string())?
        } else {
            entries_for(vault, std::slice::from_ref(&item.id), true)?
                .into_iter()
                // Only the primary key of an item and logins hold secret material; model ids and
                // endpoint URLs are plain.
                .map(|entry| {
                    let secret = match item.value_type.as_str() {
                        "login" => true,
                        _ => entry.key == item.env_key,
                    };
                    (entry.key, entry.value, secret)
                })
                .collect()
        };
        for (key, value, secret) in secrets {
            resolved.push(Resolved {
                row: ExportRow {
                    key,
                    secret_id: item.id.clone(),
                    source_name: item.name.clone(),
                    secret,
                    display: if secret {
                        MASK.to_owned()
                    } else {
                        value.clone()
                    },
                    status: "new",
                },
                value,
            });
        }
    }
    Ok(resolved)
}

/// Marks overwrites (the name is already in the target `.env`) and internal conflicts. A name that
/// is skipped is ignored on both counts.
fn classify(
    resolved: &mut [Resolved],
    existing_env: &str,
    skip: &HashSet<String>,
) -> Result<(), String> {
    let desired: Vec<_> = resolved
        .iter()
        .map(|item| secrethub_exporter::EnvEntry::new(&item.row.key, ""))
        .collect();
    let existing: HashSet<String> = detect_conflicts(existing_env, &desired)
        .map_err(|_| "existing .env could not be parsed; fix its syntax first".to_owned())?
        .into_iter()
        .collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for item in resolved.iter() {
        *counts.entry(item.row.key.as_str()).or_default() += 1;
    }
    let duplicated: HashSet<String> = counts
        .into_iter()
        .filter(|(key, count)| *count > 1 && !skip.contains(*key))
        .map(|(key, _)| key.to_owned())
        .collect();
    for item in resolved.iter_mut() {
        item.row.status = if duplicated.contains(&item.row.key) {
            "conflict"
        } else if existing.contains(&item.row.key) {
            "overwrite"
        } else {
            "new"
        };
    }
    Ok(())
}

fn read_existing_env(directory: &Path) -> String {
    std::fs::read_to_string(directory.join(".env")).unwrap_or_default()
}

pub(crate) fn preview(
    vault: &VaultService,
    request: &ExportRequestV2,
) -> Result<ExportPreviewV2, String> {
    let directory = validate_export_directory(Path::new(&request.directory))
        .map_err(|error| error.to_string())?;
    let mut resolved = resolve_rows(vault, &request.selections)?;
    let skip: HashSet<String> = request.skip_keys.iter().cloned().collect();
    classify(&mut resolved, &read_existing_env(&directory), &skip)?;
    Ok(ExportPreviewV2 {
        rows: resolved.into_iter().map(|item| item.row).collect(),
    })
}

pub(crate) fn apply(
    vault: &VaultService,
    request: &ExportRequestV2,
) -> Result<ExportOutcomeV2, String> {
    let directory = validate_export_directory(Path::new(&request.directory))
        .map_err(|error| error.to_string())?;
    let mut resolved = resolve_rows(vault, &request.selections)?;
    let skip: HashSet<String> = request.skip_keys.iter().cloned().collect();
    classify(&mut resolved, &read_existing_env(&directory), &skip)?;
    if resolved.iter().any(|item| item.row.status == "conflict") {
        return Err("the same variable name comes from more than one selected item".to_owned());
    }
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut audited = Vec::new();
    for item in resolved {
        if skip.contains(&item.row.key) {
            skipped.push(item.row.key);
            continue;
        }
        if !audited.contains(&item.row.secret_id) {
            audited.push(item.row.secret_id.clone());
        }
        entries.push(secrethub_exporter::EnvEntry::new(item.row.key, item.value));
    }
    if entries.is_empty() {
        return Err("nothing to export".to_owned());
    }
    plan::write_project_env(
        &directory,
        &entries,
        &WriteOptions {
            ensure_gitignore: true,
            write_example: true,
            merge_existing: true,
            replace_existing: false,
        },
    )
    .map_err(|error| error.message)?;
    let display_name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    // The files are written; bookkeeping failures must not turn this into a failed export.
    let _ = vault.record_project(directory.to_string_lossy().as_ref(), display_name);
    for id in audited {
        let _ = vault.record_action("env_exported", Some(&id), "{\"source\":\"desktop\"}");
    }
    Ok(ExportOutcomeV2 {
        written: entries.into_iter().map(|entry| entry.key).collect(),
        skipped,
    })
}

#[tauri::command]
pub fn export_preview_v2(
    request: ExportRequestV2,
    state: State<'_, AppState>,
) -> Result<ExportPreviewV2, String> {
    touch_activity(&state)?;
    let guard = vault_from_state(&state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    preview(vault, &request)
}

#[tauri::command]
pub fn export_apply_v2(
    request: ExportRequestV2,
    state: State<'_, AppState>,
) -> Result<ExportOutcomeV2, String> {
    touch_activity(&state)?;
    let guard = vault_from_state(&state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    apply(vault, &request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_core::{NewGroup, NewSecretInput, NewVariable};
    use secrethub_storage::{Database, SecretAttributes};
    use std::fs;

    const FAKE_KEY: &str = "fake-api-key-do-not-leak";
    const FAKE_PASSWORD: &str = "fake-ssh-password";

    fn vault(directory: &Path) -> VaultService {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        vault
    }

    fn api_key(vault: &VaultService) -> String {
        vault
            .create_secret_with_attributes(
                NewSecretInput {
                    name: "Fake OpenAI".into(),
                    provider_id: "openai".into(),
                    env_key: "OPENAI_API_KEY".into(),
                    description: String::new(),
                    tags: Vec::new(),
                    value: FAKE_KEY.into(),
                },
                SecretAttributes {
                    model_id: "fake-model".into(),
                    model_env_key: "OPENAI_MODEL".into(),
                    ..Default::default()
                },
            )
            .unwrap()
    }

    fn group(vault: &VaultService, name: &str, host: &str) -> String {
        let variable = |name: &str, value: &str, secret: bool| NewVariable {
            name: name.into(),
            value: value.into(),
            secret,
        };
        vault
            .create_group(NewGroup {
                name: name.into(),
                category: "server".into(),
                variables: vec![
                    variable("SSH_HOST", host, false),
                    variable("SSH_PASSWORD", FAKE_PASSWORD, true),
                    variable("SSH_PORT", "22", false),
                ],
                notes: String::new(),
                tags: Vec::new(),
                favorite: false,
                ai_values: true,
            })
            .unwrap()
    }

    fn selection(id: &str, variables: Option<&[&str]>) -> ExportSelection {
        ExportSelection {
            secret_id: id.to_owned(),
            variables: variables.map(|names| names.iter().map(|n| (*n).to_owned()).collect()),
        }
    }

    fn request(
        directory: &Path,
        selections: Vec<ExportSelection>,
        skip: &[&str],
    ) -> ExportRequestV2 {
        ExportRequestV2 {
            directory: directory.to_string_lossy().into_owned(),
            selections,
            skip_keys: skip.iter().map(|k| (*k).to_owned()).collect(),
        }
    }

    fn statuses(preview: &ExportPreviewV2) -> Vec<(&str, &str)> {
        preview
            .rows
            .iter()
            .map(|row| (row.key.as_str(), row.status))
            .collect()
    }

    #[test]
    fn preview_masks_secrets_shows_plain_values_and_names_the_source() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let key = api_key(&vault);
        let server = group(&vault, "Fake server", "203.0.113.7");
        let preview = preview(
            &vault,
            &request(
                project.path(),
                vec![
                    selection(&key, None),
                    selection(&server, Some(&["SSH_HOST", "SSH_PASSWORD"])),
                ],
                &[],
            ),
        )
        .unwrap();
        let shown: Vec<_> = preview
            .rows
            .iter()
            .map(|r| {
                (
                    r.key.as_str(),
                    r.source_name.as_str(),
                    r.display.as_str(),
                    r.secret,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("OPENAI_API_KEY", "Fake OpenAI", MASK, true),
                ("OPENAI_MODEL", "Fake OpenAI", "fake-model", false),
                ("SSH_HOST", "Fake server", "203.0.113.7", false),
                ("SSH_PASSWORD", "Fake server", MASK, true),
            ]
        );
        assert!(statuses(&preview)
            .iter()
            .all(|(_, status)| *status == "new"));
        let text = serde_json::to_string(&preview).unwrap();
        assert!(!text.contains(FAKE_KEY) && !text.contains(FAKE_PASSWORD));
    }

    #[test]
    fn preview_flags_overwrites_and_internal_conflicts_and_skip_clears_them() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let first = group(&vault, "Server A", "203.0.113.7");
        let second = group(&vault, "Server B", "198.51.100.9");
        fs::write(project.path().join(".env"), "KEEP=1\nSSH_PORT=2222\n").unwrap();

        // Two groups both produce SSH_HOST: both rows are conflicts; SSH_PORT already exists.
        let clash = request(
            project.path(),
            vec![
                selection(&first, Some(&["SSH_HOST", "SSH_PORT"])),
                selection(&second, Some(&["SSH_HOST"])),
            ],
            &[],
        );
        let result = preview(&vault, &clash).unwrap();
        assert_eq!(
            statuses(&result),
            [
                ("SSH_HOST", "conflict"),
                ("SSH_PORT", "overwrite"),
                ("SSH_HOST", "conflict")
            ]
        );
        assert!(apply(&vault, &clash).is_err());
        assert!(!fs::read_to_string(project.path().join(".env"))
            .unwrap()
            .contains("SSH_HOST"));

        // Cancelling one of them (a narrower subset) makes the export possible.
        let fixed = request(
            project.path(),
            vec![
                selection(&first, Some(&["SSH_HOST", "SSH_PORT"])),
                selection(&second, Some(&["SSH_PASSWORD"])),
            ],
            &[],
        );
        assert_eq!(
            statuses(&preview(&vault, &fixed).unwrap()),
            [
                ("SSH_HOST", "new"),
                ("SSH_PORT", "overwrite"),
                ("SSH_PASSWORD", "new")
            ]
        );
    }

    #[test]
    fn apply_merges_skips_keys_records_the_project_and_protects_gitignore() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let key = api_key(&vault);
        let server = group(&vault, "Fake server", "203.0.113.7");
        fs::write(
            project.path().join(".env"),
            "KEEP=1\nSSH_PORT=2222\nOPENAI_MODEL=old\n",
        )
        .unwrap();

        let outcome = apply(
            &vault,
            &request(
                project.path(),
                vec![selection(&key, None), selection(&server, None)],
                &["SSH_PORT", "OPENAI_MODEL"],
            ),
        )
        .unwrap();
        assert_eq!(outcome.skipped, ["OPENAI_MODEL", "SSH_PORT"]);
        assert_eq!(
            outcome.written,
            ["OPENAI_API_KEY", "SSH_HOST", "SSH_PASSWORD"]
        );
        let env = fs::read_to_string(project.path().join(".env")).unwrap();
        assert!(
            env.contains("KEEP=1")
                && env.contains("SSH_PORT=2222")
                && env.contains("OPENAI_MODEL=old")
        );
        assert!(env.contains(&format!("OPENAI_API_KEY=\"{FAKE_KEY}\"")));
        assert!(env.contains(&format!("SSH_PASSWORD=\"{FAKE_PASSWORD}\"")));
        assert!(env.contains("SSH_HOST=\"203.0.113.7\""));
        let example = fs::read_to_string(project.path().join(".env.example")).unwrap();
        assert!(!example.contains(FAKE_KEY) && !example.contains(FAKE_PASSWORD));
        assert!(fs::read_to_string(project.path().join(".gitignore"))
            .unwrap()
            .contains(".env"));
        let projects = vault.list_projects().unwrap();
        assert_eq!(projects.len(), 1);
        assert!(vault
            .list_audit()
            .unwrap()
            .iter()
            .any(|event| event.operation == "env_exported"
                && event.secret_id.as_deref() == Some(server.as_str())));
        let audit = serde_json::to_string(&vault.list_audit().unwrap()).unwrap();
        assert!(!audit.contains(FAKE_KEY) && !audit.contains(FAKE_PASSWORD));
    }

    #[test]
    fn selections_are_validated() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let key = api_key(&vault);
        let server = group(&vault, "Fake server", "203.0.113.7");
        let personal = vault
            .create_item(secrethub_core::NewItem {
                item_type: "bank_card".into(),
                name: "Fake card".into(),
                fields: [("bank", "Fake Bank"), ("card_number", "4111111111111111")]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
                tags: Vec::new(),
                favorite: false,
                dev_use: false,
                env_key: String::new(),
            })
            .unwrap();
        let attempt = |selections| preview(&vault, &request(project.path(), selections, &[]));
        assert!(attempt(Vec::new()).is_err());
        assert!(attempt(vec![selection(&personal, None)]).is_err());
        assert!(attempt(vec![selection("missing", None)]).is_err());
        assert!(attempt(vec![selection(&key, Some(&["X"]))]).is_err());
        assert!(attempt(vec![selection(&server, Some(&["NOPE"]))]).is_err());
        assert!(attempt(vec![selection(&server, None), selection(&server, None)]).is_err());
        let missing = tempfile::tempdir().unwrap().path().join("gone");
        assert!(preview(&vault, &request(&missing, vec![selection(&key, None)], &[])).is_err());
        assert!(apply(
            &vault,
            &request(
                project.path(),
                vec![selection(&key, None)],
                &["OPENAI_API_KEY", "OPENAI_MODEL"]
            )
        )
        .is_err());
    }

    #[test]
    fn requests_use_camel_case_names() {
        let request: ExportRequestV2 = serde_json::from_str(
            r#"{"directory":"C:/p","selections":[{"secretId":"a"},{"secretId":"g","variables":["X"]}],"skipKeys":["K"]}"#,
        )
        .unwrap();
        assert_eq!(
            request.selections[1].variables.as_deref(),
            Some(&["X".to_owned()][..])
        );
        assert_eq!(request.skip_keys, ["K"]);
    }
}
