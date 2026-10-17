use super::caps;
use super::plan::{self, PendingPlan, PlanError, PlanPreview, PlanResult, WriteOptions};
use super::AppState;
use arboard::Clipboard;
use secrethub_core::{NewSecretInput, VaultService};
use secrethub_exporter::{
    detect_conflicts, parse_env_entries, parse_json_entries, render_env, validate_export_directory,
    write_atomic, EnvEntry,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::MutexGuard,
};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Serialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

#[derive(Debug, Deserialize)]
pub struct SecretCreateRequest {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
    pub value: String,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub model_env_key: String,
    #[serde(default)]
    pub endpoint_url: String,
    #[serde(default)]
    pub endpoint_env_key: String,
}

fn default_value_type() -> String {
    "api_key".into()
}
fn default_category() -> String {
    "other".into()
}
fn default_scope() -> String {
    "global".into()
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub directory: String,
    pub secret_ids: Vec<String>,
    pub write_example: bool,
    pub replace_existing: bool,
    pub ensure_gitignore: bool,
    /// Upsert into existing `.env`/`.env.example` instead of writing fresh files.
    #[serde(default)]
    pub merge_existing: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProfileRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub secret_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub format: String,
    pub contents: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectEnvEntry {
    pub original_key: String,
    pub key: String,
    pub value: String,
    pub source: String,
    pub secret_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectEnvSaveEntry {
    pub original_key: String,
    pub key: String,
    pub value: String,
    pub source: String,
    pub secret_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectEnvSaveRequest {
    pub directory: String,
    pub entries: Vec<ProjectEnvSaveEntry>,
}

fn vault_from_state<'a>(
    state: &'a State<'_, AppState>,
) -> Result<MutexGuard<'a, Option<VaultService>>, String> {
    state
        .vault
        .lock()
        .map_err(|_| "vault state is unavailable".to_owned())
}

fn touch_activity(state: &State<'_, AppState>) -> Result<(), String> {
    let now = super::now_seconds();
    let expired = state
        .policy
        .lock()
        .map_err(|_| "vault policy is unavailable".to_owned())?
        .is_expired(now);
    if expired {
        if let Some(vault) = vault_from_state(state)?.as_mut() {
            vault.lock();
        }
        return Err("vault auto-locked after inactivity".to_owned());
    }
    state
        .policy
        .lock()
        .map_err(|_| "vault policy is unavailable".to_owned())?
        .record_activity(now);
    Ok(())
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let _ = touch_activity(&state);
    let vault = vault_from_state(&state)?;
    let Some(vault) = vault.as_ref() else {
        return Ok(VaultStatus {
            initialized: false,
            unlocked: false,
        });
    };
    Ok(VaultStatus {
        initialized: vault.is_initialized().map_err(|error| error.to_string())?,
        unlocked: vault.is_unlocked(),
    })
}

/// Records user activity (typing/clicking in the UI) so idle auto-lock measures
/// real inactivity rather than the time since the last backend call.
#[tauri::command]
pub fn vault_touch(state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)
}

#[tauri::command]
pub fn setup_master(password: String, state: State<'_, AppState>) -> Result<(), String> {
    vault_from_state(&state)?
        .as_mut()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .setup_master(&password)
        .map_err(|error| error.to_string())?;
    state
        .policy
        .lock()
        .map_err(|_| "vault policy is unavailable".to_owned())?
        .record_activity(super::now_seconds());
    Ok(())
}

#[tauri::command]
pub fn unlock(password: String, state: State<'_, AppState>) -> Result<(), String> {
    vault_from_state(&state)?
        .as_mut()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .unlock(&password)
        .map_err(|error| error.to_string())?;
    state
        .policy
        .lock()
        .map_err(|_| "vault policy is unavailable".to_owned())?
        .record_activity(super::now_seconds());
    Ok(())
}

#[tauri::command]
pub fn lock(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(vault) = vault_from_state(&state)?.as_mut() {
        vault.lock();
    }
    Ok(())
}

#[tauri::command]
pub fn secret_list(
    query: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<secrethub_storage::SecretMetadata>, String> {
    touch_activity(&state)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    query
        .filter(|value| !value.is_empty())
        .map_or_else(
            || vault.list_metadata(),
            |value| vault.search_metadata(&value),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_create(
    request: SecretCreateRequest,
    state: State<'_, AppState>,
) -> Result<String, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .create_secret_with_attributes(
            NewSecretInput {
                name: request.name,
                provider_id: request.provider_id,
                env_key: request.env_key,
                description: request.description,
                tags: request.tags,
                value: request.value,
            },
            secrethub_storage::SecretAttributes {
                value_type: request.value_type,
                category: request.category,
                scope: request.scope,
                favorite: request.favorite,
                archived: request.archived,
                model_id: request.model_id,
                model_env_key: request.model_env_key,
                endpoint_url: request.endpoint_url,
                endpoint_env_key: request.endpoint_env_key,
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_update(
    id: String,
    request: SecretCreateRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    let input = NewSecretInput {
        name: request.name,
        provider_id: request.provider_id,
        env_key: request.env_key,
        description: request.description,
        tags: request.tags,
        value: request.value,
    };
    let attributes = secrethub_storage::SecretAttributes {
        value_type: request.value_type,
        category: request.category,
        scope: request.scope,
        favorite: request.favorite,
        archived: request.archived,
        model_id: request.model_id,
        model_env_key: request.model_env_key,
        endpoint_url: request.endpoint_url,
        endpoint_env_key: request.endpoint_env_key,
    };
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    if input.value.trim().is_empty() {
        vault
            .update_metadata_with_attributes(&id, input, attributes)
            .map_err(|error| error.to_string())
    } else {
        vault
            .update_secret_with_attributes(&id, input, attributes)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub fn secret_delete(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .delete_secret(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_copy(id: String, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let value = {
        let vault = vault_from_state(&state)?;
        vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?
            .read_secret(&id)
            .map_err(|error| error.to_string())?
    };
    copy_to_clipboard(value, 30)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .record_action("secret_copied", Some(&id), "{\"source\":\"desktop\"}")
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn copy_to_clipboard(value: String, timeout_seconds: u64) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_text(&value)
        .map_err(|error| error.to_string())?;
    let value_hash = Sha256::digest(value.as_bytes()).to_vec();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(timeout_seconds));
        let Ok(mut clipboard) = Clipboard::new() else {
            return;
        };
        let Ok(current) = clipboard.get_text() else {
            return;
        };
        if Sha256::digest(current.as_bytes()).as_slice() == value_hash.as_slice() {
            let _ = clipboard.set_text("");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn secret_copy_all(id: String, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let (metadata, value) = {
        let vault = vault_from_state(&state)?;
        let vault = vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        let metadata = vault
            .list_metadata()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "secret not found".to_owned())?;
        let value = vault.read_secret(&id).map_err(|error| error.to_string())?;
        (metadata, value)
    };
    let entries = selected_entries_from_metadata(&metadata, value);
    let copied = render_env(&entries).map_err(|error| error.to_string())?;
    copy_to_clipboard(copied, 30)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .record_action("secret_copied_all", Some(&id), "{\"source\":\"desktop\"}")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn secret_validate(id: String, state: State<'_, AppState>) -> Result<String, String> {
    touch_activity(&state)?;
    let report = check_secret(&id, &state, true).await?;
    Ok(report.validation.unwrap_or_default())
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub id: String,
    pub status: String,
    pub models_sync_status: String,
}

#[tauri::command]
pub async fn secret_validate_many(
    state: State<'_, AppState>,
) -> Result<Vec<ValidationResult>, String> {
    touch_activity(&state)?;
    let ids = {
        let vault = vault_from_state(&state)?;
        vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?
            .list_metadata()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|item| item.value_type == "api_key")
            .map(|item| item.id)
            .collect::<Vec<_>>()
    };
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        let report = check_secret(&id, &state, true).await?;
        results.push(ValidationResult {
            id,
            status: report.validation.unwrap_or_default(),
            models_sync_status: report.models_status,
        });
    }
    Ok(results)
}

#[derive(Debug, Serialize)]
pub struct ModelSyncResult {
    pub id: String,
    pub status: String,
    pub count: usize,
}

/// Re-fetches only the model list ("Sync models"); the validation status is left as is.
#[tauri::command]
pub async fn secret_sync_models(
    id: String,
    state: State<'_, AppState>,
) -> Result<ModelSyncResult, String> {
    touch_activity(&state)?;
    let report = check_secret(&id, &state, false).await?;
    Ok(ModelSyncResult {
        id,
        status: report.models_status,
        count: report.count,
    })
}

/// Stored model metadata of one key (no secret values), overrides applied.
#[tauri::command]
pub fn secret_models(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<secrethub_storage::ProviderModelRow>, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .list_provider_models(&id)
        .map_err(|error| error.to_string())
}

struct CheckReport {
    validation: Option<String>,
    models_status: String,
    count: usize,
}

/// What to store for the model list after a check.
enum ModelsWrite {
    Rows(Vec<secrethub_storage::ProviderModelRow>),
    Status(&'static str),
    Keep,
}

/// Validates the key and syncs its model list with one request path.
/// The vault lock is never held across an `.await`.
async fn check_secret(
    id: &str,
    state: &State<'_, AppState>,
    update_validation: bool,
) -> Result<CheckReport, String> {
    let (provider_id, value) = {
        let vault = vault_from_state(state)?;
        let vault = vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        let metadata = vault
            .list_metadata()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "secret not found".to_owned())?;
        (
            metadata.provider_id,
            vault.read_secret(id).map_err(|error| error.to_string())?,
        )
    };
    let endpoint = secrethub_providers::models_endpoint(&provider_id);
    let (validation, write) = match &endpoint {
        None => {
            drop(value);
            ("unsupported", ModelsWrite::Status("unsupported"))
        }
        Some(endpoint) => {
            let fetched = crate::model_sync::fetch(endpoint, &value, endpoint.lists_models).await;
            drop(value);
            plan_writes(endpoint, &provider_id, fetched, update_validation)
        }
    };
    let now = super::now_seconds() as i64;
    let (models_status, count) = {
        let guard = vault_from_state(state)?;
        let vault = guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        if update_validation {
            vault
                .update_validation_status(id, validation)
                .map_err(|error| error.to_string())?;
        }
        let (status, count) = match write {
            ModelsWrite::Rows(rows) => {
                vault
                    .replace_provider_models(id, &rows, now, "ok")
                    .map_err(|error| error.to_string())?;
                ("ok", rows.len())
            }
            ModelsWrite::Status(status) => {
                vault
                    .set_models_sync_status(id, status, now)
                    .map_err(|error| error.to_string())?;
                (status, 0)
            }
            ModelsWrite::Keep => ("", 0),
        };
        if !status.is_empty() {
            vault
                .record_action(
                    "models_synced",
                    Some(id),
                    &serde_json::json!({"source": "desktop", "status": status, "count": count})
                        .to_string(),
                )
                .map_err(|error| error.to_string())?;
        }
        (status.to_owned(), count)
    };
    Ok(CheckReport {
        validation: update_validation.then(|| validation.to_owned()),
        models_status,
        count,
    })
}

/// Pure mapping from a fetch result to the validation status and model-list write.
fn plan_writes(
    endpoint: &secrethub_providers::ModelsEndpoint,
    provider_id: &str,
    fetched: crate::model_sync::Fetched,
    update_validation: bool,
) -> (&'static str, ModelsWrite) {
    let outcome = fetched
        .http_status
        .map(|status| secrethub_providers::map_validation_status(status, endpoint.is_fallback));
    let validation = outcome.map_or("network_error", |outcome| outcome.status);
    let write = match (outcome, fetched.models) {
        (Some(outcome), _) if outcome.models_unsupported => ModelsWrite::Status("unsupported"),
        (Some(outcome), models) if outcome.status == "valid" => {
            if !endpoint.lists_models {
                ModelsWrite::Status("unsupported")
            } else if let Some(models) = models {
                ModelsWrite::Rows(crate::model_sync::build_rows(provider_id, &models))
            } else {
                ModelsWrite::Status("failed")
            }
        }
        // Key rejected or network trouble: a plain validation run keeps the last sync result.
        _ if update_validation => ModelsWrite::Keep,
        _ => ModelsWrite::Status("failed"),
    };
    (validation, write)
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|_| "invalid external URL".to_owned())?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("only HTTPS external URLs are allowed".to_owned());
    }
    open::that(parsed.as_str()).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_reveal(id: String, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let value = {
        let vault = vault_from_state(&state)?;
        vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?
            .read_secret(&id)
            .map_err(|error| error.to_string())?
    };
    rfd::MessageDialog::new()
        .set_title("SecretHub")
        .set_description(&value)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .record_action("secret_revealed", Some(&id), "{\"source\":\"desktop\"}")
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn selected_entries(
    vault: &VaultService,
    ids: &[String],
) -> Result<Vec<EnvEntry>, String> {
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    for id in ids {
        let item = metadata
            .iter()
            .find(|item| item.id == *id)
            .ok_or_else(|| "secret not found".to_owned())?;
        let value = vault.read_secret(id).map_err(|error| error.to_string())?;
        let group_start = entries.len();
        let separator_before = group_start > 0;
        if !item.env_key.trim().is_empty() {
            let mut api_entry =
                entry_with_optional_comment(&item.env_key, value.clone(), &item.description);
            if separator_before {
                api_entry = api_entry.with_separator_before();
            }
            entries.push(api_entry);
        }
        if !item.model_id.trim().is_empty() {
            let model_entry = EnvEntry::new(safe_model_env_key(item), item.model_id.clone());
            entries.push(if separator_before && entries.len() == group_start {
                model_entry.with_separator_before()
            } else {
                model_entry
            });
        }
        if !item.endpoint_url.trim().is_empty() {
            entries.push(EnvEntry::new(
                safe_endpoint_env_key(item),
                item.endpoint_url.clone(),
            ));
        }
    }
    Ok(entries)
}

pub(crate) fn selected_entries_from_metadata(
    metadata: &secrethub_storage::SecretMetadata,
    value: String,
) -> Vec<EnvEntry> {
    let mut entries = Vec::new();
    if !metadata.env_key.trim().is_empty() {
        let mut api_entry =
            entry_with_optional_comment(&metadata.env_key, value, &metadata.description);
        if !entries.is_empty() {
            api_entry = api_entry.with_separator_before();
        }
        entries.push(api_entry);
    }
    if !metadata.model_id.trim().is_empty() {
        let model_env_key = safe_model_env_key(metadata);
        entries.push(EnvEntry::new(model_env_key, metadata.model_id.clone()));
    }
    if !metadata.endpoint_url.trim().is_empty() {
        entries.push(EnvEntry::new(
            safe_endpoint_env_key(metadata),
            metadata.endpoint_url.clone(),
        ));
    }
    entries
}

fn safe_endpoint_env_key(metadata: &secrethub_storage::SecretMetadata) -> String {
    if is_env_key(&metadata.endpoint_env_key) {
        return metadata.endpoint_env_key.clone();
    }
    let provider = metadata
        .provider_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_BASE_URL", provider.trim_matches('_'))
}

fn safe_model_env_key(metadata: &secrethub_storage::SecretMetadata) -> String {
    if is_env_key(&metadata.model_env_key) {
        return metadata.model_env_key.clone();
    }
    let provider = metadata
        .provider_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_MODEL", provider.trim_matches('_'))
}

fn is_env_key(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('_' | 'A'..='Z'))
        && characters.all(|character| {
            character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit()
        })
}

fn entry_with_optional_comment(key: &str, value: String, comment: &str) -> EnvEntry {
    if comment.trim().is_empty() {
        EnvEntry::new(key, value)
    } else {
        EnvEntry::with_comment(key, value, comment)
    }
}

fn parse_import(request: &ImportRequest) -> Result<Vec<EnvEntry>, String> {
    match request.format.as_str() {
        "env" => parse_env_entries(&request.contents),
        "json" => parse_json_entries(&request.contents),
        _ => return Err("import format must be env or json".to_owned()),
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_import_preview(request: ImportRequest) -> Result<Vec<String>, String> {
    Ok(parse_import(&request)?
        .into_iter()
        .map(|entry| entry.key)
        .collect())
}

#[tauri::command]
pub fn secret_import(
    request: ImportRequest,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    if !request.confirmed {
        return Err("import requires explicit confirmation after preview".to_owned());
    }
    touch_activity(&state)?;
    let entries = parse_import(&request)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    entries
        .into_iter()
        .map(|entry| {
            vault
                .create_secret(NewSecretInput {
                    name: entry.key.clone(),
                    provider_id: "generic".to_owned(),
                    env_key: entry.key,
                    description: "Imported locally".to_owned(),
                    tags: vec!["imported".to_owned()],
                    value: entry.value,
                })
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn plan_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("mcp-pending");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

#[tauri::command]
pub fn mcp_pending_plans(app: AppHandle) -> Result<Vec<PendingPlan>, String> {
    let directory = plan_directory(&app)?;
    let mut plans = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        // Only `plan-*` files; capability suggestions (`caps-*`) have their own reader.
        let is_plan = path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(plan::valid_request_id);
        if !is_plan {
            continue;
        }
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(plan) = serde_json::from_str::<PendingPlan>(&contents) {
                plans.push(plan);
            }
        }
    }
    plans.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(plans)
}

fn result_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("mcp-results"))
}

fn pending_plan_path(app: &AppHandle, request_id: &str) -> Result<PathBuf, String> {
    if !plan::valid_request_id(request_id) {
        return Err("invalid pending plan id".to_owned());
    }
    Ok(plan_directory(app)?.join(format!("{request_id}.json")))
}

fn read_pending_plan(path: &Path, request_id: &str) -> Result<PendingPlan, String> {
    let contents = fs::read_to_string(path).map_err(|_| "pending plan not found".to_owned())?;
    let plan = serde_json::from_str::<PendingPlan>(&contents)
        .map_err(|_| "pending plan is invalid".to_owned())?;
    if plan.request_id != request_id {
        return Err("pending plan is invalid".to_owned());
    }
    Ok(plan)
}

/// Read-only review data for a pending plan; API keys are masked and never decrypted.
#[tauri::command]
pub fn mcp_plan_preview(
    request_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PlanPreview, String> {
    touch_activity(&state)?;
    let plan = read_pending_plan(&pending_plan_path(&app, &request_id)?, &request_id)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    Ok(plan::preview_plan(&plan, vault))
}

/// Approves (`confirmed`) or declines a pending plan, records the outcome in
/// `mcp-results/<requestId>.json` and removes the pending file. A locked vault
/// leaves the plan pending so it can be retried after unlocking.
#[tauri::command]
pub fn mcp_confirm_plan(
    request_id: String,
    confirmed: bool,
    skip_keys: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = pending_plan_path(&app, &request_id)?;
    let results = result_directory(&app)?;
    let finish = |result: PlanResult| -> Result<(), String> {
        plan::write_result(&results, &result).map_err(|error| error.to_string())?;
        fs::remove_file(&path).map_err(|error| error.to_string())
    };
    let plan = match read_pending_plan(&path, &request_id) {
        Ok(plan) => plan,
        Err(message) if path.exists() => {
            let error = PlanError::new("invalid_plan", message.clone());
            finish(PlanResult::failed(&request_id, &error))?;
            return Err(message);
        }
        Err(message) => return Err(message),
    };
    if !confirmed {
        return finish(PlanResult::declined(&request_id));
    }
    touch_activity(&state)?;
    let skip = skip_keys.unwrap_or_default().into_iter().collect();
    let outcome = {
        let vault = vault_from_state(&state)?;
        let vault = vault
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        plan::process_plan(&plan, &skip, vault)
    };
    match outcome {
        Ok(outcome) => finish(PlanResult::written(&request_id, outcome)),
        Err(error) if error.code == "vault_locked" => Err(error.to_string()),
        Err(error) => {
            finish(PlanResult::failed(&request_id, &error))?;
            Err(error.to_string())
        }
    }
}

/// Pending AI capability suggestions (`caps-*.json`), enriched with the stored capabilities.
/// Works while the vault is locked, just without the "current" column.
#[tauri::command]
pub fn mcp_pending_capability_suggestions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<caps::SuggestionBatch>, String> {
    let directory = plan_directory(&app)?;
    let vault = vault_from_state(&state)?;
    Ok(caps::enriched_pending(&directory, vault.as_ref()))
}

/// Applies the accepted model ids as `ai` overrides and discards the rest. Needs an unlocked vault.
#[tauri::command]
pub fn mcp_resolve_capability_suggestions(
    request_id: String,
    accepted_model_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    let directory = plan_directory(&app)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    caps::resolve(&directory, &request_id, &accepted_model_ids, vault).map(|_| ())
}

#[tauri::command]
pub fn export_preview(
    request: ExportRequest,
    state: State<'_, AppState>,
) -> Result<String, String> {
    touch_activity(&state)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let entries = selected_entries(vault, &request.secret_ids)?;
    let masked = entries
        .iter()
        .map(|entry| EnvEntry::new(&entry.key, "••••••••"))
        .collect::<Vec<_>>();
    render_env(&masked).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_conflicts(
    request: ExportRequest,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    touch_activity(&state)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let directory = validate_export_directory(Path::new(&request.directory))
        .map_err(|error| error.to_string())?;
    let existing = std::fs::read_to_string(directory.join(".env")).unwrap_or_default();
    let entries = selected_entries(vault, &request.secret_ids)?;
    detect_conflicts(&existing, &entries).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_env(request: ExportRequest, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let entries = selected_entries(vault, &request.secret_ids)?;
    plan::write_project_env(
        Path::new(&request.directory),
        &entries,
        &WriteOptions {
            ensure_gitignore: request.ensure_gitignore,
            write_example: request.write_example,
            merge_existing: request.merge_existing,
            replace_existing: request.replace_existing,
        },
    )
    .map_err(|error| error.message)?;
    for secret_id in &request.secret_ids {
        vault
            .record_action("env_exported", Some(secret_id), "{\"source\":\"desktop\"}")
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn choose_project_directory() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<secrethub_storage::StoredProfile>, String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .list_profiles()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn profile_save(request: ProfileRequest, state: State<'_, AppState>) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .save_profile(secrethub_storage::StoredProfile {
            id: request.id,
            name: request.name,
            description: request.description,
            secret_ids: request.secret_ids,
            created_at: now,
            updated_at: now,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn profile_delete(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .delete_profile(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_list(
    state: State<'_, AppState>,
) -> Result<Vec<secrethub_storage::ProjectRecord>, String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .list_projects()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn project_record(
    path: String,
    display_name: String,
    state: State<'_, AppState>,
) -> Result<secrethub_storage::ProjectRecord, String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .record_project(&path, &display_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_project_directory(path: String) -> Result<(), String> {
    let directory =
        validate_export_directory(Path::new(&path)).map_err(|error| error.to_string())?;
    open::that(directory).map_err(|error| error.to_string())
}

fn metadata_for_env_key<'a>(
    metadata: &'a [secrethub_storage::SecretMetadata],
    key: &str,
) -> Option<&'a secrethub_storage::SecretMetadata> {
    metadata.iter().find(|item| {
        item.env_key == key || safe_model_env_key(item) == key || safe_endpoint_env_key(item) == key
    })
}

fn managed_env_value(
    vault: &VaultService,
    metadata: &secrethub_storage::SecretMetadata,
    key: &str,
) -> Result<Option<String>, String> {
    if metadata.env_key == key {
        return vault
            .read_secret(&metadata.id)
            .map(Some)
            .map_err(|error| error.to_string());
    }
    if safe_model_env_key(metadata) == key && !metadata.model_id.trim().is_empty() {
        return Ok(Some(metadata.model_id.clone()));
    }
    if safe_endpoint_env_key(metadata) == key && !metadata.endpoint_url.trim().is_empty() {
        return Ok(Some(metadata.endpoint_url.clone()));
    }
    Ok(None)
}

#[tauri::command]
pub fn project_env_preview(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProjectEnvEntry>, String> {
    touch_activity(&state)?;
    let directory =
        validate_export_directory(Path::new(&path)).map_err(|error| error.to_string())?;
    let contents = fs::read_to_string(directory.join(".env")).unwrap_or_default();
    let parsed = parse_env_entries(&contents).map_err(|error| error.to_string())?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    parsed
        .into_iter()
        .map(|entry| {
            if let Some(secret) = metadata_for_env_key(&metadata, &entry.key) {
                return Ok(ProjectEnvEntry {
                    original_key: entry.key.clone(),
                    key: entry.key,
                    value: "••••••••".to_owned(),
                    source: "managed".to_owned(),
                    secret_id: Some(secret.id.clone()),
                });
            }
            Ok(ProjectEnvEntry {
                original_key: entry.key.clone(),
                key: entry.key,
                value: entry.value,
                source: "manual".to_owned(),
                secret_id: None,
            })
        })
        .collect()
}

#[tauri::command]
pub fn project_env_save(
    request: ProjectEnvSaveRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    let directory = validate_export_directory(Path::new(&request.directory))
        .map_err(|error| error.to_string())?;
    let vault = vault_from_state(&state)?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    let mut keys = std::collections::HashSet::new();
    let mut entries = Vec::with_capacity(request.entries.len());
    for item in request.entries {
        let key = item.key.trim().to_owned();
        if key.is_empty() {
            return Err("environment variable key cannot be empty".to_owned());
        }
        if !keys.insert(key.clone()) {
            return Err(format!("duplicate environment variable key: {key}"));
        }
        let value = if item.source == "managed" {
            let secret_id = item
                .secret_id
                .as_deref()
                .ok_or_else(|| "managed environment entry is missing its secret id".to_owned())?;
            let secret = metadata
                .iter()
                .find(|candidate| candidate.id == secret_id)
                .ok_or_else(|| {
                    "managed environment entry references a missing secret".to_owned()
                })?;
            managed_env_value(vault, secret, &item.original_key)?
                .ok_or_else(|| "managed environment entry is no longer available".to_owned())?
        } else {
            item.value
        };
        entries.push(EnvEntry::new(key, value));
    }
    let content = render_env(&entries).map_err(|error| error.to_string())?;
    write_atomic(&directory.join(".env"), &content, true).map_err(|error| error.to_string())?;
    let display_name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    vault
        .record_project(directory.to_string_lossy().as_ref(), display_name)
        .map_err(|error| error.to_string())?;
    vault
        .record_action("project_env_saved", None, "{\"source\":\"desktop\"}")
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn audit_list(
    state: State<'_, AppState>,
) -> Result<Vec<secrethub_storage::AuditEvent>, String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .list_audit()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<secrethub_storage::Settings, String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .get_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn settings_update(
    settings: secrethub_storage::Settings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .update_settings(settings.clone())
        .map_err(|error| error.to_string())?;
    *state
        .policy
        .lock()
        .map_err(|_| "vault policy is unavailable".to_owned())? =
        secrethub_core::AutoLockPolicy::new(
            settings.auto_lock_minutes as u64 * 60,
            super::now_seconds(),
        );
    Ok(())
}

#[tauri::command]
pub fn backup_export(path: String, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let bytes = vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .encrypted_backup()
        .map_err(|error| error.to_string())?;
    let target = Path::new(&path);
    if target.file_name().and_then(|value| value.to_str()) != Some("SecretHub.secrethub-backup") {
        return Err("backup must use .secrethub-backup extension".to_owned());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "backup parent directory is required".to_owned())?;
    let directory = validate_export_directory(parent).map_err(|error| error.to_string())?;
    std::fs::write(directory.join("SecretHub.secrethub-backup"), bytes)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{is_env_key, plan_writes, selected_entries_from_metadata, ModelsWrite};
    use crate::model_sync::Fetched;
    use secrethub_providers::{DiscoveredModel, ModelsEndpoint, Protocol};

    fn endpoint(is_fallback: bool) -> ModelsEndpoint {
        ModelsEndpoint {
            url: "https://example.com/v1/models".into(),
            protocol: Protocol::Openai,
            is_fallback,
            lists_models: true,
        }
    }

    fn fetched(status: Option<u16>, models: bool) -> Fetched {
        Fetched {
            http_status: status,
            models: models.then(|| {
                vec![DiscoveredModel {
                    id: "m1".into(),
                    ..Default::default()
                }]
            }),
        }
    }

    #[test]
    fn plans_validation_and_model_writes_from_fetch_results() {
        let (status, write) = plan_writes(&endpoint(false), "x", fetched(Some(200), true), true);
        assert_eq!(status, "valid");
        assert!(matches!(write, ModelsWrite::Rows(rows) if rows.len() == 1));
        // Parse/read failure after 2xx: still valid, sync failed.
        let (status, write) = plan_writes(&endpoint(false), "x", fetched(Some(200), false), true);
        assert_eq!(status, "valid");
        assert!(matches!(write, ModelsWrite::Status("failed")));
        // Fallback 404 is unsupported for both.
        let (status, write) = plan_writes(&endpoint(true), "x", fetched(Some(404), false), true);
        assert_eq!(status, "unsupported");
        assert!(matches!(write, ModelsWrite::Status("unsupported")));
        // Explicit URL 404 stays invalid and keeps the last sync result.
        let (status, write) = plan_writes(&endpoint(false), "x", fetched(Some(404), false), true);
        assert_eq!(status, "invalid");
        assert!(matches!(write, ModelsWrite::Keep));
        let (status, write) = plan_writes(&endpoint(false), "x", fetched(None, false), true);
        assert_eq!(status, "network_error");
        assert!(matches!(write, ModelsWrite::Keep));
        // Sync-only run marks failure instead.
        let (_, write) = plan_writes(&endpoint(false), "x", fetched(Some(401), false), false);
        assert!(matches!(write, ModelsWrite::Status("failed")));
    }

    #[test]
    fn project_copy_contains_only_env_pairs_and_comment() {
        let metadata = secrethub_storage::SecretMetadata {
            id: "secret-test".into(),
            name: "DeepSeek test".into(),
            provider_id: "deepseek".into(),
            env_key: "DEEPSEEK_API_KEY".into(),
            description: "local project only".into(),
            tags: vec!["ai".into()],
            status: "unknown".into(),
            created_at: 0,
            updated_at: 0,
            value_type: "api_key".into(),
            category: "ai".into(),
            scope: "global".into(),
            favorite: false,
            archived: false,
            model_id: "deepseek-v4.1-flash".into(),
            model_env_key: "DEEPSEEK-V4.1-FLASH".into(),
            endpoint_url: "https://api.deepseek.com".into(),
            endpoint_env_key: "DEEPSEEK_BASE_URL".into(),
            models_synced_at: 0,
            models_sync_status: String::new(),
        };
        let rendered = secrethub_exporter::render_env(&selected_entries_from_metadata(
            &metadata,
            "fixture-key".into(),
        ))
        .unwrap();
        assert!(rendered.contains("# local project only"));
        assert!(rendered.contains("DEEPSEEK_API_KEY=\"fixture-key\""));
        assert!(rendered.contains("DEEPSEEK_MODEL=\"deepseek-v4.1-flash\""));
        assert!(rendered.contains("DEEPSEEK_BASE_URL=\"https://api.deepseek.com\""));
        assert!(!rendered.contains("value_type"));
        assert!(!rendered.contains("DEEPSEEK-V4.1-FLASH=\""));
    }

    #[test]
    fn accepts_only_uppercase_environment_keys() {
        assert!(is_env_key("DEEPSEEK_MODEL"));
        assert!(!is_env_key("DEEPSEEK-V4.1-FLASH"));
    }
}
