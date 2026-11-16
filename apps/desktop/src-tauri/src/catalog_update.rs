//! Remote model-catalog updates: fetch, verify, cache and apply.
//! The request carries no key, cookie or identifier. All trust decisions live in
//! `secrethub_providers::evaluate_remote`; this module only moves bytes around.

use crate::catalog::{self, CatalogState};
use crate::AppState;
use reqwest::{redirect::Policy, Client};
use secrethub_providers::{
    embedded_public_key, evaluate_remote, CatalogOrigin, Verdict, MAX_CATALOG_BYTES,
};
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use url::Url;

/// Fixed at build time. Empty or unset means remote updates are disabled.
const CATALOG_URL: Option<&str> = option_env!("SECRETHUB_CATALOG_URL");
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SIGNATURE_BYTES: usize = 4096;
const CACHE_FILE: &str = "model-catalog.json";
const CACHE_SIGNATURE_FILE: &str = "model-catalog.json.sig";

pub fn configured_url() -> Option<&'static str> {
    CATALOG_URL.map(str::trim).filter(|url| !url.is_empty())
}

/// HTTPS only, with a host and no embedded credentials.
pub fn parse_catalog_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "catalog URL is invalid".to_owned())?;
    if url.scheme() != "https" {
        return Err("catalog URL must use HTTPS".to_owned());
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("catalog URL must have a host and no credentials".to_owned());
    }
    Ok(url)
}

fn signature_url(url: &Url) -> Url {
    let mut signature = url.clone();
    let path = format!("{}.sig", url.path());
    signature.set_path(&path);
    signature
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckError {
    /// Stable machine-readable reason: url, network, status, too_large, signature, rollback, invalid, io.
    pub code: &'static str,
    pub detail: String,
}

impl CheckError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutcome {
    Updated { serial: u64 },
    UpToDate,
}

fn classify(message: &str) -> &'static str {
    if message.contains("rollback") {
        "rollback"
    } else if message.contains("signature") || message.contains("public key") {
        "signature"
    } else {
        "invalid"
    }
}

/// Fetches at most `cap` bytes without following redirects.
async fn fetch_capped(client: &Client, url: &Url, cap: usize) -> Result<Vec<u8>, CheckError> {
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| CheckError::new("network", "request failed"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CheckError::new(
            "status",
            format!("HTTP {}", status.as_u16()),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > cap as u64)
    {
        return Err(CheckError::new(
            "too_large",
            "response exceeds the size limit",
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| CheckError::new("network", "read failed"))?
    {
        if body.len() + chunk.len() > cap {
            return Err(CheckError::new(
                "too_large",
                "response exceeds the size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub fn build_client(https_only: bool) -> Result<Client, String> {
    Client::builder()
        .redirect(Policy::none())
        .timeout(FETCH_TIMEOUT)
        .https_only(https_only)
        .user_agent("SecretHub-Catalog")
        .build()
        .map_err(|_| "HTTP client unavailable".to_owned())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)
}

fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("catalog")
}

/// Verifies and applies a catalog fetched from `url` (already parsed; the caller decides
/// which schemes are acceptable).
pub async fn check_and_apply_url(
    client: &Client,
    url: &Url,
    data_dir: &Path,
    state: &CatalogState,
    public_key: &str,
) -> Result<CheckOutcome, CheckError> {
    let bytes = fetch_capped(client, url, MAX_CATALOG_BYTES).await?;
    let signature = fetch_capped(client, &signature_url(url), MAX_SIGNATURE_BYTES).await?;
    let signature = String::from_utf8(signature)
        .map_err(|_| CheckError::new("signature", "signature is not text"))?;
    let applied = state.current().serial;
    match evaluate_remote(&bytes, &signature, public_key, Some(applied)) {
        Err(error) => Err(CheckError::new(classify(&error.0), error.0)),
        Ok(Verdict::UpToDate) => Ok(CheckOutcome::UpToDate),
        Ok(Verdict::Apply(overlay)) => {
            let dir = cache_dir(data_dir);
            write_atomic(&dir.join(CACHE_FILE), &bytes)
                .and_then(|()| write_atomic(&dir.join(CACHE_SIGNATURE_FILE), signature.as_bytes()))
                .map_err(|error| CheckError::new("io", error.to_string()))?;
            state.apply(Some(&overlay));
            catalog::write_effective_models(
                &catalog::models_path(data_dir),
                &catalog::models_view(state),
            )
            .map_err(|error| CheckError::new("io", error.to_string()))?;
            Ok(CheckOutcome::Updated {
                serial: overlay.serial,
            })
        }
    }
}

pub async fn check_and_apply(
    raw_url: &str,
    data_dir: &Path,
    state: &CatalogState,
) -> Result<CheckOutcome, CheckError> {
    let url = parse_catalog_url(raw_url).map_err(|message| CheckError::new("url", message))?;
    let client = build_client(true).map_err(|message| CheckError::new("network", message))?;
    check_and_apply_url(&client, &url, data_dir, state, &embedded_public_key()).await
}

/// Re-verifies the cached file at startup (works offline). A cache that fails
/// verification, or that is not newer than the bundled data, is ignored.
pub fn load_cached(data_dir: &Path, state: &CatalogState, public_key: &str) -> bool {
    let dir = cache_dir(data_dir);
    let (Ok(bytes), Ok(signature)) = (
        fs::read(dir.join(CACHE_FILE)),
        fs::read_to_string(dir.join(CACHE_SIGNATURE_FILE)),
    ) else {
        return false;
    };
    match evaluate_remote(&bytes, &signature, public_key, None) {
        Ok(Verdict::Apply(overlay)) => {
            state.apply(Some(&overlay));
            true
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCheck {
    pub at: u64,
    /// updated | up_to_date | failed
    pub result: &'static str,
    pub code: Option<&'static str>,
    pub detail: String,
}

static LAST_CHECK: Mutex<Option<LastCheck>> = Mutex::new(None);
static CHECK_LOCK: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub serial: u64,
    pub published_at: String,
    pub source: CatalogOrigin,
    pub url_configured: bool,
    pub auto_update: bool,
    pub last_check: Option<LastCheck>,
}

fn auto_update_enabled(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let Ok(guard) = state.vault.lock() else {
        return true;
    };
    guard
        .as_ref()
        .and_then(|vault| vault.catalog_auto_update().ok())
        .unwrap_or(true)
}

fn status(app: &AppHandle) -> CatalogStatus {
    let current = catalog::state().current();
    CatalogStatus {
        serial: current.serial,
        published_at: current.published_at.clone(),
        source: current.origin,
        url_configured: configured_url().is_some(),
        auto_update: auto_update_enabled(app),
        last_check: LAST_CHECK.lock().ok().and_then(|guard| guard.clone()),
    }
}

/// One check; serialized so startup, the 24h timer and the button never overlap.
async fn run_check(app: &AppHandle) -> Result<CheckOutcome, CheckError> {
    let Some(url) = configured_url() else {
        return Err(CheckError::new(
            "not_configured",
            "no update URL configured",
        ));
    };
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| CheckError::new("io", error.to_string()))?;
    let _guard = CHECK_LOCK.lock().await;
    let result = check_and_apply(url, &data_dir, catalog::state()).await;
    let record = match &result {
        Ok(CheckOutcome::Updated { .. }) => ("updated", None, String::new()),
        Ok(CheckOutcome::UpToDate) => ("up_to_date", None, String::new()),
        Err(error) => ("failed", Some(error.code), error.detail.clone()),
    };
    if let Ok(mut last) = LAST_CHECK.lock() {
        *last = Some(LastCheck {
            at: crate::now_seconds(),
            result: record.0,
            code: record.1,
            detail: record.2,
        });
    }
    result
}

/// Startup: apply the verified cache, then check now and every 24 hours when enabled.
pub fn start(app: AppHandle) {
    if let Ok(data_dir) = app.path().app_data_dir() {
        if load_cached(&data_dir, catalog::state(), &embedded_public_key()) {
            let _ = catalog::write_effective_models(
                &catalog::models_path(&data_dir),
                &catalog::models_view(catalog::state()),
            );
        }
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if configured_url().is_some() && auto_update_enabled(&app) {
                let _ = run_check(&app).await;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckNowResult {
    /// updated | up_to_date | not_configured | failed
    pub result: &'static str,
    pub detail: String,
    pub status: CatalogStatus,
}

#[tauri::command]
pub fn catalog_status(app: AppHandle) -> CatalogStatus {
    status(&app)
}

#[tauri::command]
pub fn catalog_set_auto_update(enabled: bool, app: AppHandle) -> Result<CatalogStatus, String> {
    {
        let state = app.state::<AppState>();
        let guard = state
            .vault
            .lock()
            .map_err(|_| "vault state is unavailable".to_owned())?;
        guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?
            .set_catalog_auto_update(enabled)
            .map_err(|error| error.to_string())?;
    }
    Ok(status(&app))
}

#[tauri::command]
pub async fn catalog_check_now(app: AppHandle) -> CheckNowResult {
    let outcome = run_check(&app).await;
    let (result, detail) = match outcome {
        Ok(CheckOutcome::Updated { .. }) => ("updated", String::new()),
        Ok(CheckOutcome::UpToDate) => ("up_to_date", String::new()),
        Err(error) if error.code == "not_configured" => ("not_configured", String::new()),
        Err(error) => ("failed", format!("{}: {}", error.code, error.detail)),
    };
    CheckNowResult {
        result,
        detail,
        status: status(&app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    use secrethub_providers::bundled_serial;
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    // Test-only key pairs derived from fixed seeds.
    fn signer(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(key: &SigningKey) -> String {
        STANDARD.encode(key.verifying_key().to_bytes())
    }

    fn catalog_bytes(serial: u64, model: &str) -> Vec<u8> {
        format!(
            r#"{{"schema":1,"serial":{serial},"publishedAt":"2026-10-17T00:00:00Z","providers":{{"deepseek":{{"models":[{{"id":"{model}"}}]}}}}}}"#
        )
        .into_bytes()
    }

    struct Reply {
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    }

    fn ok(body: Vec<u8>) -> Reply {
        Reply {
            status: 200,
            headers: Vec::new(),
            body,
        }
    }

    /// Serves canned replies by request path on 127.0.0.1 until the process ends.
    fn serve(routes: Vec<(&'static str, Reply)>) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes: HashMap<_, _> = routes.into_iter().collect();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let text = String::from_utf8_lossy(&request).to_string();
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let _ = match routes.get(path.as_str()) {
                    Some(reply) => {
                        let mut head = format!(
                            "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                            reply.status,
                            reply.body.len()
                        );
                        for (name, value) in &reply.headers {
                            head.push_str(&format!("{name}: {value}\r\n"));
                        }
                        head.push_str("\r\n");
                        stream
                            .write_all(head.as_bytes())
                            .and_then(|()| stream.write_all(&reply.body))
                    }
                    None => stream.write_all(
                        b"HTTP/1.1 404 X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    ),
                };
            }
        });
        Url::parse(&format!("http://127.0.0.1:{port}/model-catalog.json")).unwrap()
    }

    fn signed_routes(key: &SigningKey, bytes: Vec<u8>) -> Vec<(&'static str, Reply)> {
        let signature = STANDARD.encode(key.sign(&bytes).to_bytes());
        vec![
            ("/model-catalog.json", ok(bytes)),
            ("/model-catalog.json.sig", ok(signature.into_bytes())),
        ]
    }

    fn run(
        url: &Url,
        dir: &Path,
        state: &CatalogState,
        key: &str,
    ) -> Result<CheckOutcome, CheckError> {
        let client = build_client(false).unwrap();
        tauri::async_runtime::block_on(check_and_apply_url(&client, url, dir, state, key))
    }

    fn newer() -> u64 {
        bundled_serial().serial + 1
    }

    #[test]
    fn happy_path_applies_caches_and_exports_models() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let state = CatalogState::new();
        let url = serve(signed_routes(&key, catalog_bytes(newer(), "ds-new")));
        let outcome = run(&url, dir.path(), &state, &public(&key)).unwrap();
        assert_eq!(outcome, CheckOutcome::Updated { serial: newer() });
        assert_eq!(state.current().origin, CatalogOrigin::Remote);
        assert!(dir.path().join("catalog").join(CACHE_FILE).exists());
        assert!(dir
            .path()
            .join("catalog")
            .join(CACHE_SIGNATURE_FILE)
            .exists());
        let exported = fs::read_to_string(catalog::models_path(dir.path())).unwrap();
        assert!(exported.contains("ds-new") && exported.contains(&newer().to_string()));
        // Same serial again is a no-op, not an error.
        assert_eq!(
            run(&url, dir.path(), &state, &public(&key)).unwrap(),
            CheckOutcome::UpToDate
        );
        // A fresh process reloads the verified cache offline.
        let restarted = CatalogState::new();
        assert!(load_cached(dir.path(), &restarted, &public(&key)));
        assert_eq!(restarted.current().serial, newer());
    }

    #[test]
    fn rejects_bad_signature_wrong_key_and_leaves_state_untouched() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let state = CatalogState::new();
        let bytes = catalog_bytes(newer(), "ds-new");
        // Signature belongs to different content.
        let forged = STANDARD.encode(key.sign(b"something else").to_bytes());
        let url = serve(vec![
            ("/model-catalog.json", ok(bytes.clone())),
            ("/model-catalog.json.sig", ok(forged.into_bytes())),
        ]);
        let error = run(&url, dir.path(), &state, &public(&key)).unwrap_err();
        assert_eq!(error.code, "signature");
        // Correct signature, but the app trusts another key.
        let url = serve(signed_routes(&key, bytes));
        let error = run(&url, dir.path(), &state, &public(&signer(2))).unwrap_err();
        assert_eq!(error.code, "signature");
        assert_eq!(state.current().origin, CatalogOrigin::Bundled);
        assert!(!dir.path().join("catalog").exists());
    }

    #[test]
    fn rejects_serial_rollback() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let state = CatalogState::new();
        let newest = serve(signed_routes(&key, catalog_bytes(newer() + 10, "ds-new")));
        run(&newest, dir.path(), &state, &public(&key)).unwrap();
        let older = serve(signed_routes(&key, catalog_bytes(newer() + 1, "ds-old")));
        assert_eq!(
            run(&older, dir.path(), &state, &public(&key))
                .unwrap_err()
                .code,
            "rollback"
        );
        assert_eq!(state.current().serial, newer() + 10);
        // Older than the bundled serial is rejected on a fresh install too.
        let below = serve(signed_routes(
            &key,
            catalog_bytes(bundled_serial().serial - 1, "x"),
        ));
        let fresh = CatalogState::new();
        assert_eq!(
            run(&below, dir.path(), &fresh, &public(&key))
                .unwrap_err()
                .code,
            "rollback"
        );
    }

    #[test]
    fn rejects_oversize_bodies_and_whitelist_violations() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let state = CatalogState::new();
        let big = vec![b' '; MAX_CATALOG_BYTES + 1];
        let url = serve(signed_routes(&key, big));
        assert_eq!(
            run(&url, dir.path(), &state, &public(&key))
                .unwrap_err()
                .code,
            "too_large"
        );
        let with_url = format!(
            r#"{{"schema":1,"serial":{},"publishedAt":"2026-10-17T00:00:00Z","providers":{{"deepseek":{{"models":[{{"id":"x","baseUrl":"https://evil.example"}}]}}}}}}"#,
            newer()
        )
        .into_bytes();
        let url = serve(signed_routes(&key, with_url));
        assert_eq!(
            run(&url, dir.path(), &state, &public(&key))
                .unwrap_err()
                .code,
            "invalid"
        );
        assert_eq!(state.current().origin, CatalogOrigin::Bundled);
    }

    #[test]
    fn does_not_follow_redirects_and_reports_http_errors() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let state = CatalogState::new();
        let url = serve(vec![(
            "/model-catalog.json",
            Reply {
                status: 302,
                headers: vec![("Location", "http://127.0.0.1:9/elsewhere".to_owned())],
                body: Vec::new(),
            },
        )]);
        let error = run(&url, dir.path(), &state, &public(&key)).unwrap_err();
        assert_eq!((error.code, error.detail.as_str()), ("status", "HTTP 302"));
        let missing = serve(Vec::new());
        assert_eq!(
            run(&missing, dir.path(), &state, &public(&key))
                .unwrap_err()
                .code,
            "status"
        );
    }

    #[test]
    fn only_https_urls_without_credentials_are_accepted() {
        assert!(parse_catalog_url("https://cdn.example.com/model-catalog.json").is_ok());
        for bad in [
            "http://cdn.example.com/model-catalog.json",
            "ftp://cdn.example.com/x",
            "https://user:pw@cdn.example.com/x",
            "not a url",
            "",
        ] {
            assert!(parse_catalog_url(bad).is_err(), "{bad}");
        }
        let signature = signature_url(
            &parse_catalog_url("https://cdn.example.com/a/model-catalog.json").unwrap(),
        );
        assert_eq!(
            signature.as_str(),
            "https://cdn.example.com/a/model-catalog.json.sig"
        );
        let state = CatalogState::new();
        let dir = tempfile::tempdir().unwrap();
        let error = tauri::async_runtime::block_on(check_and_apply(
            "http://127.0.0.1/model-catalog.json",
            dir.path(),
            &state,
        ))
        .unwrap_err();
        assert_eq!(error.code, "url");
    }

    #[test]
    fn tampered_or_stale_cache_is_ignored_at_startup() {
        let key = signer(1);
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("catalog");
        fs::create_dir_all(&cache).unwrap();
        let state = CatalogState::new();
        // Valid signature but not newer than the bundled serial: ignored.
        let stale = catalog_bytes(bundled_serial().serial, "ds-stale");
        fs::write(cache.join(CACHE_FILE), &stale).unwrap();
        fs::write(
            cache.join(CACHE_SIGNATURE_FILE),
            STANDARD.encode(key.sign(&stale).to_bytes()),
        )
        .unwrap();
        assert!(!load_cached(dir.path(), &state, &public(&key)));
        // Fresh file edited after signing: ignored.
        let fresh = catalog_bytes(newer(), "ds-new");
        let signature = STANDARD.encode(key.sign(&fresh).to_bytes());
        fs::write(cache.join(CACHE_SIGNATURE_FILE), signature).unwrap();
        fs::write(cache.join(CACHE_FILE), catalog_bytes(newer(), "ds-evil")).unwrap();
        assert!(!load_cached(dir.path(), &state, &public(&key)));
        assert_eq!(state.current().origin, CatalogOrigin::Bundled);
        // Missing files: ignored.
        assert!(!load_cached(
            tempfile::tempdir().unwrap().path(),
            &state,
            &public(&key)
        ));
    }
}
