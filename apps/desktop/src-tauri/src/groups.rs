//! Commands for config groups (`env_group`). Secret variables only ever leave through a native
//! dialog, the clipboard or a key file the user picks; plain values may be shown. The notes
//! text is returned to the web view only when the user asks to read or edit it.

use super::commands::{copy_to_clipboard, touch_activity, vault_from_state};
use super::AppState;
use secrethub_core::{
    GroupPublic, GroupUpdate, NewGroup, NewVariable, VariableUpdate, VaultService,
};
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use tauri::State;

const CLIPBOARD_SECONDS: u64 = 30;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupVariableCreate {
    pub name: String,
    #[serde(default)]
    pub value: String,
    pub secret: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCreateRequest {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub variables: Vec<GroupVariableCreate>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_true")]
    pub ai_values: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupVariableChange {
    pub name: String,
    pub secret: bool,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdateRequest {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub variables: Vec<GroupVariableChange>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_true")]
    pub ai_values: bool,
}

fn default_true() -> bool {
    true
}

fn vault_error(error: secrethub_core::VaultError) -> String {
    error.to_string()
}

fn with_vault<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&VaultService) -> Result<T, String>,
) -> Result<T, String> {
    touch_activity(state)?;
    let guard = vault_from_state(state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    action(vault)
}

fn audit(vault: &VaultService, operation: &str, id: &str) -> Result<(), String> {
    vault
        .record_action(
            operation,
            Some(id),
            "{\"source\":\"desktop\",\"type\":\"env_group\"}",
        )
        .map_err(vault_error)
}

/// The value of a secret variable, or an error when the variable is plain.
fn secret_value(vault: &VaultService, id: &str, name: &str) -> Result<String, String> {
    let secret = vault
        .group_variables(id)
        .map_err(vault_error)?
        .into_iter()
        .any(|row| row.name == name && row.secret);
    if !secret {
        return Err("variable is not a secret variable".to_owned());
    }
    vault.group_variable_value(id, name).map_err(vault_error)
}

#[tauri::command]
pub fn group_create(
    request: GroupCreateRequest,
    state: State<'_, AppState>,
) -> Result<String, String> {
    with_vault(&state, |vault| {
        vault
            .create_group(NewGroup {
                name: request.name,
                category: request.category,
                variables: request
                    .variables
                    .into_iter()
                    .map(|variable| NewVariable {
                        name: variable.name,
                        value: variable.value,
                        secret: variable.secret,
                    })
                    .collect(),
                notes: request.notes,
                tags: request.tags,
                favorite: request.favorite,
                ai_values: request.ai_values,
            })
            .map_err(vault_error)
    })
}

#[tauri::command]
pub fn group_update(
    id: String,
    request: GroupUpdateRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_vault(&state, |vault| {
        vault
            .update_group(
                &id,
                GroupUpdate {
                    name: request.name,
                    category: request.category,
                    variables: request
                        .variables
                        .into_iter()
                        .map(|variable| VariableUpdate {
                            name: variable.name,
                            secret: variable.secret,
                            value: variable.value,
                            from: variable.from,
                        })
                        .collect(),
                    notes: request.notes,
                    tags: request.tags,
                    favorite: request.favorite,
                    ai_values: request.ai_values,
                },
            )
            .map_err(vault_error)
    })
}

/// Variable names, secret flags and plain values; secret values are never part of it.
#[tauri::command]
pub fn group_public(id: String, state: State<'_, AppState>) -> Result<GroupPublic, String> {
    with_vault(&state, |vault| vault.group_public(&id).map_err(vault_error))
}

/// Shows one secret variable in a native dialog; the value never reaches the web view.
#[tauri::command]
pub fn group_var_reveal(
    id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let value = with_vault(&state, |vault| secret_value(vault, &id, &name))?;
    rfd::MessageDialog::new()
        .set_title("SecretHub")
        .set_description(value)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    with_vault(&state, |vault| audit(vault, "group_var_revealed", &id))
}

/// Copies one variable with the usual auto-clearing clipboard; the web view never sees it.
#[tauri::command]
pub fn group_var_copy(id: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    let value = with_vault(&state, |vault| {
        vault.group_variable_value(&id, &name).map_err(vault_error)
    })?;
    copy_to_clipboard(value, CLIPBOARD_SECONDS)?;
    with_vault(&state, |vault| audit(vault, "group_var_copied", &id))
}

/// The one exception to "secrets stay out of the web view": the notes text, only when the user
/// asks to show or edit it.
#[tauri::command]
pub fn group_notes(id: String, state: State<'_, AppState>) -> Result<String, String> {
    with_vault(&state, |vault| {
        let notes = vault.group_notes(&id).map_err(vault_error)?;
        audit(vault, "group_notes_read", &id)?;
        Ok(notes)
    })
}

/// Native save dialog for `group_export_key_file`; `None` when the user cancels.
#[tauri::command]
pub fn group_choose_key_path(suggested: String) -> Option<String> {
    rfd::FileDialog::new()
        .set_file_name(&suggested)
        .save_file()
        .map(|path| path.to_string_lossy().into_owned())
}

/// Writes a secret variable (an SSH key, say) to a new file. Never overwrites.
pub(crate) fn write_key_file(path: &Path, value: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("key file path must be absolute".to_owned());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "file already exists".to_owned()
            } else {
                format!("cannot write key file: {}", error.kind())
            }
        })?;
    file.write_all(value.as_bytes())
        .and_then(|()| {
            if value.ends_with('\n') {
                Ok(())
            } else {
                file.write_all(b"\n")
            }
        })
        .map_err(|error| format!("cannot write key file: {}", error.kind()))
}

#[tauri::command]
pub fn group_export_key_file(
    id: String,
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let value = with_vault(&state, |vault| secret_value(vault, &id, &name))?;
    write_key_file(Path::new(&path), &value)?;
    with_vault(&state, |vault| audit(vault, "group_key_exported", &id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_camel_case_and_default_ai_values_on() {
        let create: GroupCreateRequest = serde_json::from_str(
            r#"{"name":"Fake","category":"server","variables":[{"name":"SSH_HOST","value":"h","secret":false},{"name":"SSH_PASSWORD","secret":true}]}"#,
        )
        .unwrap();
        assert!(create.ai_values && create.notes.is_empty());
        assert_eq!(create.variables[1].value, "");
        let update: GroupUpdateRequest = serde_json::from_str(
            r#"{"name":"Fake","category":"server","aiValues":false,"variables":[{"name":"B","secret":true,"from":"A"}]}"#,
        )
        .unwrap();
        assert!(!update.ai_values && update.notes.is_none());
        assert_eq!(update.variables[0].from.as_deref(), Some("A"));
        assert!(update.variables[0].value.is_none());
    }

    #[test]
    fn key_files_are_created_once_and_end_with_a_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id_fake");
        let key =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----";
        write_key_file(&path, key).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{key}\n"));
        let error = write_key_file(&path, "other").unwrap_err();
        assert_eq!(error, "file already exists");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{key}\n"));
        assert!(write_key_file(Path::new("relative-key"), "x").is_err());
        let with_newline = dir.path().join("second");
        write_key_file(&with_newline, "abc\n").unwrap();
        assert_eq!(std::fs::read_to_string(with_newline).unwrap(), "abc\n");
    }
}
