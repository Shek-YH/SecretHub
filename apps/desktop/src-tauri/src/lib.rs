use secrethub_core::{AutoLockPolicy, VaultService};
use secrethub_storage::Database;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

mod commands;

pub struct AppState {
    pub vault: Mutex<Option<VaultService>>,
    pub policy: Mutex<AutoLockPolicy>,
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
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
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::setup_master,
            commands::unlock,
            commands::lock,
            commands::secret_list,
            commands::secret_create,
            commands::secret_update,
            commands::secret_delete,
            commands::secret_import_preview,
            commands::secret_import,
            commands::mcp_pending_plans,
            commands::mcp_confirm_plan,
            commands::secret_copy,
            commands::secret_copy_all,
            commands::secret_reveal,
            commands::secret_validate,
            commands::secret_validate_many,
            commands::open_external_url,
            commands::export_preview,
            commands::export_conflicts,
            commands::export_env,
            commands::choose_project_directory,
            commands::profile_list,
            commands::profile_save,
            commands::profile_delete,
            commands::project_list,
            commands::project_record,
            commands::open_project_directory,
            commands::project_env_preview,
            commands::project_env_save,
            commands::audit_list,
            commands::settings_get,
            commands::settings_update,
            commands::backup_export
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
