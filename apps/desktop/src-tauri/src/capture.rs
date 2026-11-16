//! Clipboard key capture (design M3). After the user agrees (their own click, or "Start" on an
//! AI request) the official key page opens in their browser and the clipboard is polled for up
//! to three minutes for a *new* value that looks like an API key. The value lives only in this
//! process: it is never logged, never returned, never sent anywhere except the provider's
//! built-in address by the usual validate+sync, and is dropped right after the secret is stored.

use crate::{catalog, commands, queue, sync, AppState};
use arboard::Clipboard;
use secrethub_core::NewSecretInput;
use secrethub_providers::ProviderDefinition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};

const PREFIX: &str = "capture-";
const KIND: &str = "start_key_capture";
pub const CAPTURE_WINDOW: Duration = Duration::from_secs(180);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MIN_KEY_LEN: usize = 16;
const MAX_KEY_LEN: usize = 512;

// ------------------------------------------------------------------ detection

pub type TextHash = [u8; 32];

pub fn hash_text(text: &str) -> TextHash {
    Sha256::digest(text.as_bytes()).into()
}

/// Whether new clipboard text looks like an API key for a provider with these prefixes.
/// Returns the trimmed value. Rules (design M3.2): not the baseline content, a single line of
/// printable ASCII without spaces, 16–512 characters, and one of the provider's prefixes when
/// it has any.
pub fn detect_key(
    clipboard: &str,
    baseline: Option<TextHash>,
    prefixes: &[String],
) -> Option<String> {
    if baseline == Some(hash_text(clipboard)) {
        return None;
    }
    let value = clipboard.trim();
    let length = value.len();
    if !(MIN_KEY_LEN..=MAX_KEY_LEN).contains(&length)
        || !value.chars().all(|ch| ch.is_ascii_graphic())
    {
        return None;
    }
    if !prefixes.is_empty()
        && !prefixes
            .iter()
            .any(|prefix| value.starts_with(prefix.as_str()))
    {
        return None;
    }
    Some(value.to_owned())
}

fn read_clipboard() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}

/// Clears the clipboard only while it still holds the captured value.
fn clear_clipboard_if_equals(value: &str) {
    let Ok(mut clipboard) = Clipboard::new() else {
        return;
    };
    if clipboard
        .get_text()
        .is_ok_and(|current| current.trim() == value)
    {
        let _ = clipboard.set_text("");
    }
}

// ------------------------------------------------------------------- preset

/// Secret fields taken from the catalog; never contains the key itself.
#[derive(Debug, PartialEq, Eq)]
pub struct CapturePreset {
    pub name: String,
    pub env_key: String,
    pub model_id: String,
    pub model_env_key: String,
    pub endpoint_url: String,
    pub endpoint_env_key: String,
}

pub fn build_preset(provider: &ProviderDefinition, date: &str) -> CapturePreset {
    let model_id = provider
        .default_model
        .clone()
        .filter(|id| provider.models.iter().any(|model| &model.id == id))
        .or_else(|| {
            provider
                .models
                .iter()
                .find(|model| !model.deprecated)
                .map(|model| model.id.clone())
        })
        .unwrap_or_default();
    let endpoint_env_key = provider.base_url_env_key.clone().unwrap_or_else(|| {
        format!(
            "{}_BASE_URL",
            provider.id.to_ascii_uppercase().replace('-', "_")
        )
    });
    CapturePreset {
        name: format!("{} · 自动捕获 {date}", provider.name),
        env_key: provider.env_keys.first().cloned().unwrap_or_default(),
        model_id,
        model_env_key: provider
            .model_env_key
            .clone()
            .unwrap_or_else(|| "MODEL_NAME".to_owned()),
        endpoint_url: provider.base_url.clone(),
        endpoint_env_key,
    }
}

// -------------------------------------------------------------------- state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Waiting,
    Captured,
    Timeout,
    Cancelled,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Waiting => "waiting",
            Phase::Captured => "captured",
            Phase::Timeout => "timeout",
            Phase::Cancelled => "cancelled",
        }
    }
}

struct Session {
    generation: u64,
    provider_id: String,
    request_id: Option<String>,
    started: Instant,
    phase: Phase,
    secret_id: Option<String>,
    validation: Option<String>,
}

#[derive(Default)]
struct Inner {
    counter: u64,
    session: Option<Session>,
}

/// What the polling task should do after a tick.
#[derive(Debug, PartialEq, Eq)]
pub enum Tick {
    Continue,
    /// Superseded or cancelled; stop silently.
    Gone,
    TimedOut,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    /// `idle` | `waiting` | `captured` | `timeout` | `cancelled`. Never carries the key.
    pub state: &'static str,
    pub provider_id: Option<String>,
    pub remaining_seconds: u64,
    pub secret_id: Option<String>,
    /// Validation status of the captured key once checked (`valid`, `unauthorized`, ...).
    pub validation: Option<String>,
}

#[derive(Default)]
pub struct CaptureState {
    inner: Mutex<Inner>,
}

impl CaptureState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts a new waiting session. Returns its generation and the request id of a session it
    /// replaced while that was still waiting.
    pub fn begin(
        &self,
        provider_id: &str,
        request_id: Option<String>,
        now: Instant,
    ) -> (u64, Option<String>) {
        let mut inner = self.lock();
        let replaced = inner
            .session
            .take()
            .filter(|session| session.phase == Phase::Waiting)
            .and_then(|session| session.request_id);
        inner.counter += 1;
        let generation = inner.counter;
        inner.session = Some(Session {
            generation,
            provider_id: provider_id.to_owned(),
            request_id,
            started: now,
            phase: Phase::Waiting,
            secret_id: None,
            validation: None,
        });
        (generation, replaced)
    }

    pub fn tick(&self, generation: u64, now: Instant) -> Tick {
        let mut inner = self.lock();
        match inner.session.as_mut() {
            Some(session)
                if session.generation == generation && session.phase == Phase::Waiting =>
            {
                if now.saturating_duration_since(session.started) >= CAPTURE_WINDOW {
                    session.phase = Phase::Timeout;
                    Tick::TimedOut
                } else {
                    Tick::Continue
                }
            }
            _ => Tick::Gone,
        }
    }

    /// Cancels the running session; returns its provider and request id when it was waiting.
    pub fn cancel(&self) -> Option<(u64, Option<String>)> {
        let mut inner = self.lock();
        let session = inner.session.as_mut()?;
        if session.phase != Phase::Waiting {
            return None;
        }
        session.phase = Phase::Cancelled;
        Some((session.generation, session.request_id.clone()))
    }

    /// Cancels one specific session; false when it is gone, superseded or already finished.
    pub fn cancel_generation(&self, generation: u64) -> bool {
        let mut inner = self.lock();
        match inner.session.as_mut() {
            Some(session)
                if session.generation == generation && session.phase == Phase::Waiting =>
            {
                session.phase = Phase::Cancelled;
                true
            }
            _ => false,
        }
    }

    /// Marks the session captured; `None` when it was superseded or already finished.
    pub fn mark_captured(&self, generation: u64, secret_id: &str) -> Option<Option<String>> {
        let mut inner = self.lock();
        let session = inner.session.as_mut()?;
        if session.generation != generation || session.phase != Phase::Waiting {
            return None;
        }
        session.phase = Phase::Captured;
        session.secret_id = Some(secret_id.to_owned());
        Some(session.request_id.clone())
    }

    pub fn set_validation(&self, generation: u64, validation: &str) {
        if let Some(session) = self
            .lock()
            .session
            .as_mut()
            .filter(|session| session.generation == generation)
        {
            session.validation = Some(validation.to_owned());
        }
    }

    pub fn request_id(&self, generation: u64) -> Option<String> {
        self.lock()
            .session
            .as_ref()
            .filter(|session| session.generation == generation)
            .and_then(|session| session.request_id.clone())
    }

    pub fn status(&self, now: Instant) -> CaptureStatus {
        let inner = self.lock();
        let Some(session) = inner.session.as_ref() else {
            return CaptureStatus {
                state: "idle",
                provider_id: None,
                remaining_seconds: 0,
                secret_id: None,
                validation: None,
            };
        };
        let remaining = if session.phase == Phase::Waiting {
            CAPTURE_WINDOW
                .saturating_sub(now.saturating_duration_since(session.started))
                .as_secs()
        } else {
            0
        };
        CaptureStatus {
            state: session.phase.as_str(),
            provider_id: Some(session.provider_id.clone()),
            remaining_seconds: remaining,
            secret_id: session.secret_id.clone(),
            validation: session.validation.clone(),
        }
    }
}

// --------------------------------------------------------------- queue files

#[derive(Debug, Deserialize)]
struct PendingCapture {
    #[serde(default)]
    version: u32,
    #[serde(alias = "requestId")]
    request_id: String,
    kind: String,
    #[serde(alias = "providerId")]
    provider_id: String,
    #[serde(alias = "createdAt")]
    created_at: String,
}

fn registry_provider(provider_id: &str) -> Option<ProviderDefinition> {
    catalog::effective_registry()
        .into_iter()
        .find(|provider| provider.id == provider_id)
}

/// The key page must be an https URL and the provider must name an API key variable.
fn capturable(provider: &ProviderDefinition) -> Option<String> {
    let url = provider.key_url.as_deref()?;
    let parsed = url::Url::parse(url).ok()?;
    (parsed.scheme() == "https" && parsed.host_str().is_some() && !provider.env_keys.is_empty())
        .then(|| parsed.to_string())
}

impl PendingCapture {
    fn validated(self, expected_id: &str) -> Result<(Self, ProviderDefinition), String> {
        if self.request_id != expected_id || !queue::valid_id(&self.request_id, PREFIX) {
            return Err("invalid capture request: request id".to_owned());
        }
        if self.kind != KIND || self.version > 1 {
            return Err("invalid capture request: kind or version".to_owned());
        }
        let provider = registry_provider(&self.provider_id)
            .filter(|provider| capturable(provider).is_some())
            .ok_or_else(|| "invalid capture request: provider".to_owned())?;
        Ok((self, provider))
    }
}

fn read_pending(directory: &std::path::Path) -> Vec<(PendingCapture, ProviderDefinition)> {
    let mut requests = queue::pending_files(directory, PREFIX)
        .into_iter()
        .filter_map(|(id, path)| {
            serde_json::from_str::<PendingCapture>(&fs::read_to_string(path).ok()?)
                .ok()?
                .validated(&id)
                .ok()
        })
        .collect::<Vec<_>>();
    requests.sort_by(|left, right| left.0.created_at.cmp(&right.0.created_at));
    requests
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureResult<'a> {
    request_id: &'a str,
    /// `waiting` | `captured` | `timeout` | `cancelled` | `declined`; the secret id only on `captured`.
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret_id: Option<&'a str>,
    updated_at: String,
}

fn directories(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok((data.join("mcp-pending"), data.join("mcp-results")))
}

fn write_request_result(app: &AppHandle, request_id: &str, status: &str, secret_id: Option<&str>) {
    let Ok((_, results)) = directories(app) else {
        return;
    };
    let _ = queue::write_result(
        &results,
        request_id,
        PREFIX,
        &CaptureResult {
            request_id,
            status,
            secret_id,
            updated_at: chrono::Utc::now().to_rfc3339(),
        },
    );
}

// ---------------------------------------------------------------- the task

fn audit(app: &AppHandle, operation: &str, secret_id: Option<&str>, provider_id: &str) {
    let state = app.state::<AppState>();
    let guard = state.vault.lock();
    if let Some(vault) = guard.as_ref().ok().and_then(|guard| guard.as_ref()) {
        let _ = vault.record_action(
            operation,
            secret_id,
            &serde_json::json!({"source": "clipboard", "provider": provider_id}).to_string(),
        );
    }
}

/// Stores the captured value as a secret from the provider preset; the value is dropped here.
fn store_captured(
    state: &State<'_, AppState>,
    provider: &ProviderDefinition,
    value: String,
) -> Result<String, String> {
    let preset = build_preset(
        provider,
        &chrono::Local::now().format("%Y-%m-%d").to_string(),
    );
    let guard = state
        .vault
        .lock()
        .map_err(|_| "vault state is unavailable".to_owned())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    vault
        .create_secret_with_attributes(
            NewSecretInput {
                name: preset.name,
                provider_id: provider.id.clone(),
                env_key: preset.env_key,
                description: String::new(),
                tags: Vec::new(),
                value,
            },
            secrethub_storage::SecretAttributes {
                category: "ai".into(),
                model_id: preset.model_id,
                model_env_key: preset.model_env_key,
                endpoint_url: preset.endpoint_url,
                endpoint_env_key: preset.endpoint_env_key,
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())
}

fn finish_unsuccessful(app: &AppHandle, generation: u64, status: &str) {
    let state = app.state::<AppState>();
    if let Some(request_id) = state.capture.request_id(generation) {
        write_request_result(app, &request_id, status, None);
    }
}

async fn run_session(
    app: AppHandle,
    generation: u64,
    provider: ProviderDefinition,
    baseline: Option<TextHash>,
) {
    let state = app.state::<AppState>();
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        match state.capture.tick(generation, Instant::now()) {
            Tick::Continue => {}
            Tick::Gone => return,
            Tick::TimedOut => {
                finish_unsuccessful(&app, generation, "timeout");
                return;
            }
        }
        // An auto-lock while waiting ends the capture: nothing may be stored into a locked vault.
        if !sync::ready(&state) {
            if state.capture.cancel_generation(generation) {
                finish_unsuccessful(&app, generation, "cancelled");
            }
            return;
        }
        let Some(text) = read_clipboard() else {
            continue;
        };
        let Some(value) = detect_key(&text, baseline, &provider.prefixes) else {
            continue;
        };
        drop(text);
        let stored = store_captured(&state, &provider, value.clone());
        let secret_id = match stored {
            Ok(id) => id,
            Err(_) => {
                if state.capture.cancel_generation(generation) {
                    finish_unsuccessful(&app, generation, "cancelled");
                }
                return;
            }
        };
        clear_clipboard_if_equals(&value);
        drop(value);
        let Some(request_id) = state.capture.mark_captured(generation, &secret_id) else {
            return;
        };
        if let Some(request_id) = request_id {
            write_request_result(&app, &request_id, "captured", Some(&secret_id));
        }
        audit(&app, "key_captured", Some(&secret_id), &provider.id);
        let validation = match commands::check_secret(&secret_id, &state, true).await {
            Ok(report) => report.validation.unwrap_or_default(),
            Err(_) => "network_error".to_owned(),
        };
        state.capture.set_validation(generation, &validation);
        return;
    }
}

/// Opens the key page and starts waiting for a copied key. `request_id` links an AI request.
fn start_capture(
    app: &AppHandle,
    provider_id: &str,
    request_id: Option<String>,
) -> Result<CaptureStatus, String> {
    let state = app.state::<AppState>();
    if !sync::ready(&state) {
        return Err("vault is locked".to_owned());
    }
    let provider = registry_provider(provider_id).ok_or_else(|| "unknown provider".to_owned())?;
    let key_url = capturable(&provider).ok_or_else(|| "provider has no key page".to_owned())?;
    // Only content that appears after this point counts; the old content is kept as a hash.
    let baseline = read_clipboard().map(|text| hash_text(&text));
    let (generation, replaced) = state.capture.begin(provider_id, request_id, Instant::now());
    if let Some(replaced) = replaced {
        write_request_result(app, &replaced, "cancelled", None);
    }
    if let Err(error) = commands::open_external_url(key_url) {
        if state.capture.cancel_generation(generation) {
            finish_unsuccessful(app, generation, "cancelled");
        }
        return Err(error);
    }
    audit(app, "key_capture_started", None, provider_id);
    tauri::async_runtime::spawn(run_session(app.clone(), generation, provider, baseline));
    Ok(state.capture.status(Instant::now()))
}

// ----------------------------------------------------------------- commands

/// User-initiated capture: opens the official key page and waits for a copied key.
#[tauri::command]
pub fn key_capture_start(provider_id: String, app: AppHandle) -> Result<CaptureStatus, String> {
    start_capture(&app, &provider_id, None)
}

#[tauri::command]
pub fn key_capture_cancel(app: AppHandle, state: State<'_, AppState>) -> CaptureStatus {
    if let Some((generation, _)) = state.capture.cancel() {
        finish_unsuccessful(&app, generation, "cancelled");
    }
    state.capture.status(Instant::now())
}

/// Status and countdown only; never the key.
#[tauri::command]
pub fn key_capture_status(state: State<'_, AppState>) -> CaptureStatus {
    state.capture.status(Instant::now())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRequestView {
    pub request_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub created_at: String,
}

/// AI capture requests waiting for the user's "Start" or "Decline"; needs no vault access.
#[tauri::command]
pub fn mcp_pending_capture_requests(app: AppHandle) -> Result<Vec<CaptureRequestView>, String> {
    let (pending, _) = directories(&app)?;
    Ok(read_pending(&pending)
        .into_iter()
        .map(|(request, provider)| CaptureRequestView {
            request_id: request.request_id,
            provider_id: provider.id,
            provider_name: provider.name,
            created_at: request.created_at,
        })
        .collect())
}

/// "Start" or "Decline" on an AI capture request. Declining needs no vault; starting does, and
/// a locked vault leaves the request pending.
#[tauri::command]
pub fn mcp_capture_decision(
    request_id: String,
    accept: bool,
    app: AppHandle,
) -> Result<CaptureStatus, String> {
    if !queue::valid_id(&request_id, PREFIX) {
        return Err("invalid capture request id".to_owned());
    }
    let (pending, _) = directories(&app)?;
    let path = pending.join(format!("{request_id}.json"));
    let contents = fs::read_to_string(&path).map_err(|_| "capture request not found".to_owned())?;
    let (request, provider) = serde_json::from_str::<PendingCapture>(&contents)
        .map_err(|_| "capture request is invalid".to_owned())?
        .validated(&request_id)?;
    let state = app.state::<AppState>();
    if !accept {
        write_request_result(&app, &request.request_id, "declined", None);
        fs::remove_file(&path).map_err(|error| error.to_string())?;
        return Ok(state.capture.status(Instant::now()));
    }
    let status = start_capture(&app, &provider.id, Some(request.request_id.clone()))?;
    write_request_result(&app, &request.request_id, "waiting", None);
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Obviously fake values; never a real key shape.
    const FAKE_KEY: &str = "fake-test-key-0123456789abcdef";
    const FAKE_PREFIXED: &str = "sk-fake-0123456789abcdef";

    fn prefixes(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn accepts_a_single_printable_line_of_plausible_length() {
        assert_eq!(detect_key(FAKE_KEY, None, &[]).as_deref(), Some(FAKE_KEY));
        // Surrounding whitespace and a trailing newline are trimmed.
        assert_eq!(
            detect_key(&format!("  {FAKE_KEY}\r\n"), None, &[]).as_deref(),
            Some(FAKE_KEY)
        );
        assert!(detect_key(&"a".repeat(16), None, &[]).is_some());
        assert!(detect_key(&"a".repeat(512), None, &[]).is_some());
    }

    #[test]
    fn rejects_bad_length_charset_and_multiline_text() {
        assert!(detect_key(&"a".repeat(15), None, &[]).is_none());
        assert!(detect_key(&"a".repeat(513), None, &[]).is_none());
        assert!(detect_key("", None, &[]).is_none());
        assert!(detect_key("fake test key with spaces 0123456789", None, &[]).is_none());
        assert!(detect_key("fake-test-key-line-one\nfake-test-key-line-two", None, &[]).is_none());
        assert!(detect_key("fake-test-key-0123456789\u{4e2d}\u{6587}", None, &[]).is_none());
        assert!(detect_key("fake-test-key-\t0123456789abcdef", None, &[]).is_none());
    }

    #[test]
    fn prefixes_must_match_when_the_provider_has_any() {
        let sk = prefixes(&["sk-fake-", "sk-other-"]);
        assert!(detect_key(FAKE_PREFIXED, None, &sk).is_some());
        assert!(detect_key(FAKE_KEY, None, &sk).is_none());
        assert!(detect_key(FAKE_KEY, None, &[]).is_some());
    }

    #[test]
    fn ignores_the_baseline_content_but_not_new_content() {
        let baseline = Some(hash_text(FAKE_KEY));
        assert!(detect_key(FAKE_KEY, baseline, &[]).is_none());
        assert!(detect_key(FAKE_PREFIXED, baseline, &[]).is_some());
        // The baseline compares the raw text, so a re-copy with different whitespace is new.
        assert!(detect_key(&format!("{FAKE_KEY}\n"), baseline, &[]).is_some());
    }

    #[test]
    fn session_state_machine_counts_down_and_times_out() {
        let state = CaptureState::default();
        assert_eq!(state.status(Instant::now()).state, "idle");
        let start = Instant::now();
        let (generation, replaced) = state.begin("deepseek", Some("capture-1".into()), start);
        assert!(replaced.is_none());
        assert_eq!(
            state.tick(generation, start + Duration::from_secs(10)),
            Tick::Continue
        );
        let status = state.status(start + Duration::from_secs(10));
        assert_eq!((status.state, status.remaining_seconds), ("waiting", 170));
        assert_eq!(
            state.tick(generation, start + CAPTURE_WINDOW),
            Tick::TimedOut
        );
        assert_eq!(state.status(start + CAPTURE_WINDOW).state, "timeout");
        assert_eq!(state.tick(generation, start + CAPTURE_WINDOW), Tick::Gone);
        assert!(state.mark_captured(generation, "secret-1").is_none());
    }

    #[test]
    fn cancel_capture_and_supersede_rules() {
        let state = CaptureState::default();
        let now = Instant::now();
        let (first, _) = state.begin("deepseek", Some("capture-a".into()), now);
        // A new session replaces a waiting one and reports the replaced request.
        let (second, replaced) = state.begin("openai", None, now);
        assert_eq!(replaced.as_deref(), Some("capture-a"));
        assert_eq!(state.tick(first, now), Tick::Gone);
        assert!(state.mark_captured(first, "secret-x").is_none());
        assert!(!state.cancel_generation(first));
        assert_eq!(
            state.cancel().map(|(generation, _)| generation),
            Some(second)
        );
        assert_eq!(state.status(now).state, "cancelled");
        assert!(state.cancel().is_none());
        let (third, _) = state.begin("openai", Some("capture-c".into()), now);
        assert_eq!(
            state.mark_captured(third, "secret-1"),
            Some(Some("capture-c".into()))
        );
        state.set_validation(third, "valid");
        let status = state.status(now);
        assert_eq!(status.state, "captured");
        assert_eq!(status.secret_id.as_deref(), Some("secret-1"));
        assert_eq!(status.validation.as_deref(), Some("valid"));
        assert_eq!(status.remaining_seconds, 0);
    }

    fn provider(json: &str) -> ProviderDefinition {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn preset_comes_from_the_catalog_entry() {
        let deepseek = provider(
            r#"{"id":"deepseek","name":"DeepSeek","baseUrl":"https://api.deepseek.com","envKeys":["DEEPSEEK_API_KEY"],"baseUrlEnvKey":"DEEPSEEK_BASE_URL","modelEnvKey":"DEEPSEEK_MODEL","keyUrl":"https://platform.deepseek.com/api_keys","defaultModel":"m-pro","models":[{"id":"m-flash"},{"id":"m-pro"}]}"#,
        );
        assert_eq!(
            build_preset(&deepseek, "2026-10-28"),
            CapturePreset {
                name: "DeepSeek · 自动捕获 2026-10-28".into(),
                env_key: "DEEPSEEK_API_KEY".into(),
                model_id: "m-pro".into(),
                model_env_key: "DEEPSEEK_MODEL".into(),
                endpoint_url: "https://api.deepseek.com".into(),
                endpoint_env_key: "DEEPSEEK_BASE_URL".into(),
            }
        );
        let bare = provider(
            r#"{"id":"my-agg","name":"Agg","baseUrl":"https://agg.example/v1","envKeys":["AGG_API_KEY"],"models":[{"id":"old","deprecated":true},{"id":"new"}]}"#,
        );
        let preset = build_preset(&bare, "2026-10-28");
        assert_eq!(preset.model_id, "new");
        assert_eq!(preset.model_env_key, "MODEL_NAME");
        assert_eq!(preset.endpoint_env_key, "MY_AGG_BASE_URL");
    }

    #[test]
    fn only_https_key_pages_of_key_providers_are_capturable() {
        let base = r#""id":"p","name":"P","baseUrl":"https://p.example","models":[]"#;
        let ok = provider(&format!(
            r#"{{{base},"envKeys":["P_KEY"],"keyUrl":"https://p.example/keys"}}"#
        ));
        assert_eq!(capturable(&ok).as_deref(), Some("https://p.example/keys"));
        let http = provider(&format!(
            r#"{{{base},"envKeys":["P_KEY"],"keyUrl":"http://p.example/keys"}}"#
        ));
        assert!(capturable(&http).is_none());
        let no_url = provider(&format!(r#"{{{base},"envKeys":["P_KEY"]}}"#));
        assert!(capturable(&no_url).is_none());
        let no_env = provider(&format!(
            r#"{{{base},"envKeys":[],"keyUrl":"https://p.example"}}"#
        ));
        assert!(capturable(&no_env).is_none());
    }

    #[test]
    fn pending_requests_need_a_known_capturable_provider_and_other_readers_ignore_them() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, provider: &str| {
            let id = name.trim_end_matches(".json");
            fs::write(
                dir.path().join(name),
                format!(
                    r#"{{"version":1,"requestId":"{id}","kind":"start_key_capture","providerId":"{provider}","createdAt":"2026-10-28T00:00:00Z"}}"#
                ),
            )
            .unwrap();
        };
        write("capture-ok.json", "deepseek");
        write("capture-unknown.json", "no-such-provider");
        write("capture-custom.json", "custom");
        fs::write(dir.path().join("sync-1.json"), "{}").unwrap();
        let found = read_pending(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.request_id, "capture-ok");
        assert_eq!(found[0].1.id, "deepseek");
        assert!(crate::sync::read_pending(dir.path()).is_empty());
        assert!(crate::caps::read_pending(dir.path()).is_empty());
        assert!(!crate::plan::valid_request_id("capture-ok"));
    }

    #[test]
    fn result_files_name_the_secret_only_when_captured() {
        let dir = tempfile::tempdir().unwrap();
        let write = |status: &str, secret: Option<&str>| {
            queue::write_result(
                dir.path(),
                "capture-r1",
                PREFIX,
                &CaptureResult {
                    request_id: "capture-r1",
                    status,
                    secret_id: secret,
                    updated_at: "now".into(),
                },
            )
            .unwrap();
            serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(dir.path().join("capture-r1.json")).unwrap(),
            )
            .unwrap()
        };
        let declined = write("declined", None);
        assert_eq!(declined["status"], "declined");
        assert!(declined.get("secretId").is_none());
        assert_eq!(write("captured", Some("secret-1"))["secretId"], "secret-1");
    }
}
