//! Silent in-app updates (tauri-plugin-updater, NSIS "quiet" install mode).
//! Background loop: check ~3 s after start and then hourly, download silently, keep the verified
//! installer in memory and tell the UI. The user either clicks "Update" (silent install + automatic
//! restart) or does nothing, in which case the installer runs silently when the app exits.
//! Background failures are swallowed; only the manual check reports its result.

use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(3);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
const READY_EVENT: &str = "update-ready";

struct Ready {
    update: Update,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub struct UpdateState {
    ready: Mutex<Option<Ready>>,
    busy: AtomicBool,
}

impl UpdateState {
    fn ready_version(&self) -> Option<String> {
        self.ready
            .lock()
            .ok()?
            .as_ref()
            .map(|ready| ready.update.version.clone())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub ready_version: Option<String>,
    pub current_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckNowResult {
    /// up_to_date | ready | busy | failed
    pub result: &'static str,
    pub version: Option<String>,
    pub detail: String,
}

impl CheckNowResult {
    fn new(result: &'static str, version: Option<String>, detail: impl Into<String>) -> Self {
        Self {
            result,
            version,
            detail: detail.into(),
        }
    }
}

/// Check for an update and, if one exists, download it into memory (signature is verified by the plugin).
async fn check_and_download(app: &AppHandle) -> CheckNowResult {
    let state = app.state::<UpdateState>();
    if let Some(version) = state.ready_version() {
        return CheckNowResult::new("ready", Some(version), "");
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return CheckNowResult::new("busy", None, "");
    }
    let outcome = download_latest(app).await;
    state.busy.store(false, Ordering::SeqCst);
    outcome
}

async fn download_latest(app: &AppHandle) -> CheckNowResult {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(error) => return CheckNowResult::new("failed", None, error.to_string()),
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return CheckNowResult::new("up_to_date", None, ""),
        Err(error) => return CheckNowResult::new("failed", None, error.to_string()),
    };
    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(bytes) => bytes,
        Err(error) => return CheckNowResult::new("failed", None, error.to_string()),
    };
    let version = update.version.clone();
    if let Ok(mut slot) = app.state::<UpdateState>().ready.lock() {
        *slot = Some(Ready { update, bytes });
    }
    let _ = app.emit(READY_EVENT, &version);
    CheckNowResult::new("ready", Some(version), "")
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            // Failures are ignored on purpose: the next hourly check retries.
            let _ = check_and_download(&app).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    UpdateStatus {
        ready_version: app.state::<UpdateState>().ready_version(),
        current_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
pub async fn update_check_now(app: AppHandle) -> CheckNowResult {
    check_and_download(&app).await
}

/// Run the downloaded installer silently; it restarts the app when done. Does not return on success.
#[tauri::command]
pub fn update_install(app: AppHandle) -> Result<(), String> {
    let ready = app
        .state::<UpdateState>()
        .ready
        .lock()
        .map_err(|_| "update state is unavailable".to_owned())?
        .take()
        .ok_or_else(|| "no downloaded update".to_owned())?;
    ready.update.install(&ready.bytes).map_err(|error| error.to_string())
}

/// On quit with a downloaded update that the user never applied: install silently and do not relaunch.
pub fn install_on_exit(app: &AppHandle) {
    let Some(state) = app.try_state::<UpdateState>() else {
        return;
    };
    let ready = state.ready.lock().ok().and_then(|mut slot| slot.take());
    if let Some(ready) = ready {
        let _ = ready
            .update
            .restart_after_install(false)
            .install(&ready.bytes);
    }
}
