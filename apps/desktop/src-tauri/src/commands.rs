use super::AppState;
use arboard::Clipboard;
use secrethub_core::{NewSecretInput, VaultService};
use secrethub_exporter::{detect_conflicts, ensure_gitignore_env, env_is_tracked, gitignore_has_env, render_env, render_example, validate_export_directory, write_atomic, EnvEntry};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::MutexGuard};
use tauri::State;

#[derive(Debug, Serialize)]
pub struct VaultStatus { pub initialized: bool, pub unlocked: bool }

#[derive(Debug, Deserialize)]
pub struct SecretCreateRequest { pub name: String, pub provider_id: String, pub env_key: String, pub description: String, pub tags: Vec<String>, pub value: String }

#[derive(Debug, Deserialize)]
pub struct ExportRequest { pub directory: String, pub secret_ids: Vec<String>, pub write_example: bool, pub replace_existing: bool, pub ensure_gitignore: bool }

#[derive(Debug, Deserialize)]
pub struct ProfileRequest { pub id: String, pub name: String, pub description: String, pub secret_ids: Vec<String> }

fn vault_from_state<'a>(state: &'a State<'_, AppState>) -> Result<MutexGuard<'a, Option<VaultService>>, String> { state.vault.lock().map_err(|_| "vault state is unavailable".to_owned()) }

fn touch_activity(state: &State<'_, AppState>) -> Result<(), String> {
    let now = super::now_seconds();
    let expired = state.policy.lock().map_err(|_| "vault policy is unavailable".to_owned())?.is_expired(now);
    if expired { if let Some(vault) = vault_from_state(state)?.as_mut() { vault.lock(); } return Err("vault auto-locked after inactivity".to_owned()); }
    state.policy.lock().map_err(|_| "vault policy is unavailable".to_owned())?.record_activity(now);
    Ok(())
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let _ = touch_activity(&state);
    let vault = vault_from_state(&state)?;
    let Some(vault) = vault.as_ref() else { return Ok(VaultStatus { initialized: false, unlocked: false }); };
    Ok(VaultStatus { initialized: vault.is_initialized().map_err(|error| error.to_string())?, unlocked: vault.is_unlocked() })
}

#[tauri::command]
pub fn setup_master(password: String, state: State<'_, AppState>) -> Result<(), String> { vault_from_state(&state)?.as_mut().ok_or_else(|| "vault unavailable".to_owned())?.setup_master(&password).map_err(|error| error.to_string())?; state.policy.lock().map_err(|_| "vault policy is unavailable".to_owned())?.record_activity(super::now_seconds()); Ok(()) }

#[tauri::command]
pub fn unlock(password: String, state: State<'_, AppState>) -> Result<(), String> { vault_from_state(&state)?.as_mut().ok_or_else(|| "vault unavailable".to_owned())?.unlock(&password).map_err(|error| error.to_string())?; state.policy.lock().map_err(|_| "vault policy is unavailable".to_owned())?.record_activity(super::now_seconds()); Ok(()) }

#[tauri::command]
pub fn lock(state: State<'_, AppState>) -> Result<(), String> { if let Some(vault) = vault_from_state(&state)?.as_mut() { vault.lock(); } Ok(()) }

#[tauri::command]
pub fn secret_list(query: Option<String>, state: State<'_, AppState>) -> Result<Vec<secrethub_storage::SecretMetadata>, String> { touch_activity(&state)?; let vault = vault_from_state(&state)?; let vault = vault.as_ref().ok_or_else(|| "vault unavailable".to_owned())?; query.filter(|value| !value.is_empty()).map_or_else(|| vault.list_metadata(), |value| vault.search_metadata(&value)).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn secret_create(request: SecretCreateRequest, state: State<'_, AppState>) -> Result<String, String> { touch_activity(&state)?; vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.create_secret(NewSecretInput { name: request.name, provider_id: request.provider_id, env_key: request.env_key, description: request.description, tags: request.tags, value: request.value }).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn secret_update(id: String, request: SecretCreateRequest, state: State<'_, AppState>) -> Result<(), String> { touch_activity(&state)?; vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.update_secret(&id, NewSecretInput { name: request.name, provider_id: request.provider_id, env_key: request.env_key, description: request.description, tags: request.tags, value: request.value }).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn secret_delete(id: String, state: State<'_, AppState>) -> Result<bool, String> { touch_activity(&state)?; vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.delete_secret(&id).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn secret_copy(id: String, state: State<'_, AppState>) -> Result<(), String> { touch_activity(&state)?; let value = vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.read_secret(&id).map_err(|error| error.to_string())?; let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?; clipboard.set_text(value).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn secret_reveal(id: String, state: State<'_, AppState>) -> Result<(), String> { touch_activity(&state)?; let value = vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.read_secret(&id).map_err(|error| error.to_string())?; rfd::MessageDialog::new().set_title("SecretHub").set_description(&value).set_buttons(rfd::MessageButtons::Ok).show(); Ok(()) }

fn selected_entries(vault: &VaultService, ids: &[String]) -> Result<Vec<EnvEntry>, String> { let metadata = vault.list_metadata().map_err(|error| error.to_string())?; ids.iter().map(|id| { let item = metadata.iter().find(|item| item.id == *id).ok_or_else(|| "secret not found".to_owned())?; let value = vault.read_secret(id).map_err(|error| error.to_string())?; Ok(EnvEntry::new(&item.env_key, value)) }).collect() }

#[tauri::command]
pub fn export_preview(request: ExportRequest, state: State<'_, AppState>) -> Result<String, String> { touch_activity(&state)?; let vault = vault_from_state(&state)?; let vault = vault.as_ref().ok_or_else(|| "vault unavailable".to_owned())?; let entries = selected_entries(vault, &request.secret_ids)?; let masked = entries.iter().map(|entry| EnvEntry::new(&entry.key, "••••••••")).collect::<Vec<_>>(); render_env(&masked).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn export_conflicts(request: ExportRequest, state: State<'_, AppState>) -> Result<Vec<String>, String> { touch_activity(&state)?; let vault = vault_from_state(&state)?; let vault = vault.as_ref().ok_or_else(|| "vault unavailable".to_owned())?; let directory = validate_export_directory(Path::new(&request.directory)).map_err(|error| error.to_string())?; let existing = std::fs::read_to_string(directory.join(".env")).unwrap_or_default(); let entries = selected_entries(vault, &request.secret_ids)?; detect_conflicts(&existing, &entries).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn export_env(request: ExportRequest, state: State<'_, AppState>) -> Result<(), String> { touch_activity(&state)?; let vault = vault_from_state(&state)?; let vault = vault.as_ref().ok_or_else(|| "vault unavailable".to_owned())?; let directory = validate_export_directory(Path::new(&request.directory)).map_err(|error| error.to_string())?; if env_is_tracked(&directory).map_err(|error| error.to_string())? { return Err(".env is already tracked by Git; remediation required before export".to_owned()); } let gitignore = directory.join(".gitignore"); if !gitignore_has_env(&gitignore).map_err(|error| error.to_string())? { if request.ensure_gitignore { ensure_gitignore_env(&gitignore).map_err(|error| error.to_string())?; } else { return Err(".env is not protected by .gitignore; explicit remediation required".to_owned()); } } let entries = selected_entries(vault, &request.secret_ids)?; let content = render_env(&entries).map_err(|error| error.to_string())?; write_atomic(&directory.join(".env"), &content, request.replace_existing).map_err(|error| error.to_string())?; if request.write_example { let example = render_example(&entries).map_err(|error| error.to_string())?; write_atomic(&directory.join(".env.example"), &example, request.replace_existing).map_err(|error| error.to_string())?; } Ok(()) }

#[tauri::command]
pub fn choose_project_directory() -> Option<String> { rfd::FileDialog::new().pick_folder().map(|path| path.to_string_lossy().into_owned()) }

#[tauri::command]
pub fn profile_list(state: State<'_, AppState>) -> Result<Vec<secrethub_storage::StoredProfile>, String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.list_profiles().map_err(|error| error.to_string()) }

#[tauri::command]
pub fn profile_save(request: ProfileRequest, state: State<'_, AppState>) -> Result<(), String> { let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64; vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.save_profile(secrethub_storage::StoredProfile { id: request.id, name: request.name, description: request.description, secret_ids: request.secret_ids, created_at: now, updated_at: now }).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn project_list(state: State<'_, AppState>) -> Result<Vec<secrethub_storage::ProjectRecord>, String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.list_projects().map_err(|error| error.to_string()) }

#[tauri::command]
pub fn project_record(path: String, display_name: String, state: State<'_, AppState>) -> Result<secrethub_storage::ProjectRecord, String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.record_project(&path, &display_name).map_err(|error| error.to_string()) }

#[tauri::command]
pub fn audit_list(state: State<'_, AppState>) -> Result<Vec<secrethub_storage::AuditEvent>, String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.list_audit().map_err(|error| error.to_string()) }

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<secrethub_storage::Settings, String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.get_settings().map_err(|error| error.to_string()) }

#[tauri::command]
pub fn settings_update(settings: secrethub_storage::Settings, state: State<'_, AppState>) -> Result<(), String> { vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.update_settings(settings.clone()).map_err(|error| error.to_string())?; *state.policy.lock().map_err(|_| "vault policy is unavailable".to_owned())? = secrethub_core::AutoLockPolicy::new(settings.auto_lock_minutes as u64 * 60, super::now_seconds()); Ok(()) }

#[tauri::command]
pub fn backup_export(path: String, state: State<'_, AppState>) -> Result<(), String> { let bytes = vault_from_state(&state)?.as_ref().ok_or_else(|| "vault unavailable".to_owned())?.encrypted_backup().map_err(|error| error.to_string())?; let target = Path::new(&path); if target.extension().and_then(|value| value.to_str()) != Some("secrethub-backup") { return Err("backup must use .secrethub-backup extension".to_owned()); } std::fs::write(target, bytes).map_err(|error| error.to_string()) }
