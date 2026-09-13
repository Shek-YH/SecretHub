use super::AppState;
use arboard::Clipboard;
use secrethub_core::{NewSecretInput, VaultService};
use secrethub_exporter::{
    detect_conflicts, ensure_gitignore_env, env_is_tracked, gitignore_has_env, parse_env_entries,
    parse_json_entries, render_env, render_example, validate_export_directory, write_atomic,
    EnvEntry,
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
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub directory: String,
    pub secret_ids: Vec<String>,
    pub write_example: bool,
    pub replace_existing: bool,
    pub ensure_gitignore: bool,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PendingPlan {
    pub request_id: String,
    pub kind: String,
    pub project_path: String,
    pub secret_ids: Vec<String>,
    pub created_at: String,
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
        .create_secret(NewSecretInput {
            name: request.name,
            provider_id: request.provider_id,
            env_key: request.env_key,
            description: request.description,
            tags: request.tags,
            value: request.value,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn secret_update(
    id: String,
    request: SecretCreateRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .update_secret(
            &id,
            NewSecretInput {
                name: request.name,
                provider_id: request.provider_id,
                env_key: request.env_key,
                description: request.description,
                tags: request.tags,
                value: request.value,
            },
        )
        .map_err(|error| error.to_string())
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
    let value = vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .read_secret(&id)
        .map_err(|error| error.to_string())?;
    let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_text(&value)
        .map_err(|error| error.to_string())?;
    let value_hash = Sha256::digest(value.as_bytes()).to_vec();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
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
pub fn secret_reveal(id: String, state: State<'_, AppState>) -> Result<(), String> {
    touch_activity(&state)?;
    let value = vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .read_secret(&id)
        .map_err(|error| error.to_string())?;
    rfd::MessageDialog::new()
        .set_title("SecretHub")
        .set_description(&value)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    Ok(())
}

fn selected_entries(vault: &VaultService, ids: &[String]) -> Result<Vec<EnvEntry>, String> {
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    ids.iter()
        .map(|id| {
            let item = metadata
                .iter()
                .find(|item| item.id == *id)
                .ok_or_else(|| "secret not found".to_owned())?;
            let value = vault.read_secret(id).map_err(|error| error.to_string())?;
            Ok(EnvEntry::new(&item.env_key, value))
        })
        .collect()
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
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(plan) = serde_json::from_str::<PendingPlan>(&contents) {
                plans.push(plan);
            }
        }
    }
    plans.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(plans)
}

#[tauri::command]
pub fn mcp_confirm_plan(
    request_id: String,
    confirmed: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !confirmed {
        return Err("plan confirmation was declined".to_owned());
    }
    if !request_id.starts_with("plan-") || request_id.contains(['/', '\\', '.']) {
        return Err("invalid pending plan id".to_owned());
    }
    let path = plan_directory(&app)?.join(format!("{request_id}.json"));
    let contents = fs::read_to_string(&path).map_err(|_| "pending plan not found".to_owned())?;
    let plan: PendingPlan =
        serde_json::from_str(&contents).map_err(|_| "pending plan is invalid".to_owned())?;
    if plan.request_id != request_id
        || plan.secret_ids.is_empty()
        || plan.project_path.trim().is_empty()
    {
        return Err("pending plan is invalid".to_owned());
    }
    export_env(
        ExportRequest {
            directory: plan.project_path,
            secret_ids: plan.secret_ids,
            write_example: true,
            replace_existing: false,
            ensure_gitignore: true,
        },
        state,
    )?;
    fs::remove_file(path).map_err(|error| error.to_string())?;
    Ok(())
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
    let directory = validate_export_directory(Path::new(&request.directory))
        .map_err(|error| error.to_string())?;
    if env_is_tracked(&directory).map_err(|error| error.to_string())? {
        return Err(
            ".env is already tracked by Git; remediation required before export".to_owned(),
        );
    }
    let gitignore = directory.join(".gitignore");
    if !gitignore_has_env(&gitignore).map_err(|error| error.to_string())? {
        if request.ensure_gitignore {
            ensure_gitignore_env(&gitignore).map_err(|error| error.to_string())?;
        } else {
            return Err(
                ".env is not protected by .gitignore; explicit remediation required".to_owned(),
            );
        }
    }
    let entries = selected_entries(vault, &request.secret_ids)?;
    let content = render_env(&entries).map_err(|error| error.to_string())?;
    write_atomic(&directory.join(".env"), &content, request.replace_existing)
        .map_err(|error| error.to_string())?;
    if request.write_example {
        let example = render_example(&entries).map_err(|error| error.to_string())?;
        write_atomic(
            &directory.join(".env.example"),
            &example,
            request.replace_existing,
        )
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
