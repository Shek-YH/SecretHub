use secrethub_core::VaultService;
use secrethub_storage::Database;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

mod commands;

pub struct AppState { pub vault: Mutex<Option<VaultService>> }

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Ok(data_dir.join("vault.sqlite"))
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = database_path(app.handle()).map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            let database = Database::open(path).map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let vault = VaultService::open(database).map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            app.manage(AppState { vault: Mutex::new(Some(vault)) });
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
            commands::secret_copy,
            commands::secret_reveal,
            commands::export_preview,
            commands::export_env,
            commands::choose_project_directory,
            commands::profile_list,
            commands::profile_save,
            commands::project_list,
            commands::project_record,
            commands::audit_list,
            commands::settings_get,
            commands::settings_update,
            commands::backup_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running SecretHub");
}
