use secrethub_core::{AutoLockPolicy, VaultService};
use secrethub_storage::Database;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

mod caps;
mod capture;
mod catalog;
mod catalog_update;
mod commands;
mod csv_import;
#[cfg(test)]
mod e2e_journey;
mod export;
mod groups;
mod import;
mod import_flow;
mod items;
mod mask;
mod media;
mod model_sync;
mod organize;
#[cfg(test)]
mod organize_fixture;
mod paste;
mod plan;
mod queue;
mod restore;
mod scan;
mod sync;

pub struct AppState {
    pub vault: Mutex<Option<VaultService>>,
    pub policy: Mutex<AutoLockPolicy>,
    pub capture: capture::CaptureState,
    pub imports: import_flow::Sessions,
    pub inspections: mask::Inspections,
}

pub(crate) fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Ok(data_dir.join("vault.sqlite"))
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = database_path(app.handle()).map_err(Box::<dyn std::error::Error>::from)?;
            if let Some(data_dir) = path.parent() {
                // Best effort: MCP falls back to the bundled catalog when this file is missing.
                let _ = catalog::write_effective_models(
                    &catalog::models_path(data_dir),
                    &catalog::current_models(),
                );
            }
            let database = Database::open(path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let vault = VaultService::open(database)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let settings = vault
                .get_settings()
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            app.manage(AppState {
                vault: Mutex::new(Some(vault)),
                policy: Mutex::new(AutoLockPolicy::new(
                    settings.auto_lock_minutes as u64 * 60,
                    now_seconds(),
                )),
                capture: capture::CaptureState::default(),
                imports: import_flow::Sessions::default(),
                inspections: mask::Inspections::default(),
            });
            catalog_update::start(app.handle().clone());
            sync::start(app.handle().clone());
            organize::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_touch,
            commands::setup_master,
            commands::unlock,
            commands::lock,
            commands::vault_change_password,
            items::item_create,
            items::item_update,
            items::item_public_fields,
            items::item_secret_fields,
            items::item_field_reveal,
            items::item_field_copy,
            items::item_note_body,
            groups::group_create,
            groups::group_update,
            groups::group_public,
            groups::group_var_reveal,
            groups::group_var_copy,
            groups::group_notes,
            groups::group_choose_key_path,
            groups::group_export_key_file,
            commands::secret_list,
            commands::secret_create,
            commands::secret_update,
            commands::secret_delete,
            commands::secret_import_preview,
            commands::secret_import,
            commands::choose_import_file,
            import_flow::import_scan,
            import_flow::import_apply,
            import_flow::import_cancel,
            import_flow::import_scan_text,
            import_flow::import_choose_files,
            import_flow::import_save_bank_template,
            import_flow::mcp_pending_import_requests,
            import_flow::mcp_import_decline,
            organize::mcp_pending_organize_requests,
            organize::ai_import_notices,
            organize::ai_import_dismiss,
            organize::ai_import_undo,
            commands::secret_import_file_preview,
            commands::secret_import_file,
            commands::mcp_pending_plans,
            commands::mcp_confirm_plan,
            commands::mcp_plan_preview,
            commands::mcp_pending_capability_suggestions,
            commands::mcp_resolve_capability_suggestions,
            commands::secret_copy,
            commands::secret_copy_all,
            commands::secret_reveal,
            commands::secret_validate,
            commands::secret_validate_many,
            commands::secret_sync_models,
            commands::secret_models,
            commands::secret_capability_models,
            commands::secret_set_capability_models,
            commands::open_external_url,
            commands::export_preview,
            commands::export_conflicts,
            commands::export_env,
            export::export_preview_v2,
            export::export_apply_v2,
            commands::choose_project_directory,
            commands::profile_list,
            commands::profile_save,
            commands::profile_delete,
            commands::tag_list,
            commands::tag_create,
            commands::tag_delete,
            commands::project_list,
            commands::project_record,
            commands::open_project_directory,
            commands::project_env_preview,
            commands::project_env_save,
            commands::audit_list,
            commands::catalog_effective,
            sync::mcp_pending_sync_requests,
            capture::key_capture_start,
            capture::key_capture_cancel,
            capture::key_capture_status,
            capture::mcp_pending_capture_requests,
            capture::mcp_capture_decision,
            media::media_list,
            media::media_choose_files,
            media::media_import,
            media::media_create_text,
            media::media_update,
            media::media_delete,
            media::media_copy_path,
            media::media_reveal,
            media::media_thumbnail,
            media::mcp_pending_media_requests,
            media::mcp_media_decision,
            catalog_update::catalog_status,
            catalog_update::catalog_set_auto_update,
            catalog_update::catalog_check_now,
            commands::settings_get,
            commands::settings_update,
            commands::backup_export,
            commands::choose_backup_file,
            commands::backup_restore
        ])
        .run(tauri::generate_context!())
        .expect("error while running SecretHub");
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
