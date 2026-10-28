//! AI-requested model sync (design M2): `sync-*.json` files queued by the MCP server.
//! They hold secret ids only. Desktop runs the same validate+sync path as the Check button as
//! soon as the vault is unlocked (no confirmation: keys only go to their built-in provider
//! address), then writes `mcp-results/sync-<id>.json` with model ids and counts, never values.

use crate::commands;
use crate::queue;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};

const PREFIX: &str = "sync-";
const KIND: &str = "sync_models";
const MAX_SECRET_IDS: usize = 100;
const MAX_LISTED_MODELS: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const KEY_SPACING: Duration = Duration::from_secs(1);
const DEDUPE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct PendingSync {
    #[serde(default)]
    pub version: u32,
    #[serde(alias = "requestId")]
    pub request_id: String,
    pub kind: String,
    /// Absent means every non-archived API key with a stored value.
    #[serde(default, alias = "secretIds")]
    pub secret_ids: Option<Vec<String>>,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

impl PendingSync {
    /// Same rules as the MCP tool; messages never echo file content.
    fn validated(self, expected_id: &str) -> Result<Self, String> {
        if self.request_id != expected_id || !queue::valid_id(&self.request_id, PREFIX) {
            return Err("invalid sync request: request id".to_owned());
        }
        if self.kind != KIND || self.version > 1 {
            return Err("invalid sync request: kind or version".to_owned());
        }
        if let Some(ids) = &self.secret_ids {
            if ids.is_empty()
                || ids.len() > MAX_SECRET_IDS
                || ids
                    .iter()
                    .any(|id| id.is_empty() || id.len() > 200 || id.chars().any(char::is_control))
            {
                return Err("invalid sync request: secret ids".to_owned());
            }
        }
        Ok(self)
    }
}

/// Valid `sync-*.json` requests only, oldest first; other prefixes and malformed files are ignored.
pub fn read_pending(directory: &Path) -> Vec<PendingSync> {
    let mut requests = queue::pending_files(directory, PREFIX)
        .into_iter()
        .filter_map(|(id, path)| {
            let contents = fs::read_to_string(path).ok()?;
            serde_json::from_str::<PendingSync>(&contents)
                .ok()?
                .validated(&id)
                .ok()
        })
        .collect::<Vec<_>>();
    requests.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    requests
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSecretResult {
    pub secret_id: String,
    pub provider_id: String,
    /// `ok` | `failed` | `unsupported` | `unchanged` (key rejected, list kept) | `skipped`.
    pub models_sync_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    pub count: usize,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub added_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncResult<'a> {
    request_id: &'a str,
    status: &'static str,
    secrets: &'a [SyncSecretResult],
    finished_at: String,
}

/// Model ids added to / removed from the previous set, sorted (listing capped; counts are exact).
pub fn diff_models(before: &[String], after: &[String]) -> (Vec<String>, Vec<String>) {
    let previous: BTreeSet<&String> = before.iter().collect();
    let current: BTreeSet<&String> = after.iter().collect();
    let added = current.difference(&previous).map(|id| (*id).clone());
    let removed = previous.difference(&current).map(|id| (*id).clone());
    (added.collect(), removed.collect())
}

/// Results of the last 60 seconds per key; a repeated request reuses them instead of hitting the provider.
#[derive(Default)]
pub struct RecentResults {
    entries: HashMap<String, (Instant, SyncSecretResult)>,
}

impl RecentResults {
    pub fn get(&self, secret_id: &str, now: Instant) -> Option<&SyncSecretResult> {
        self.entries
            .get(secret_id)
            .filter(|(at, _)| now.saturating_duration_since(*at) < DEDUPE_WINDOW)
            .map(|(_, result)| result)
    }

    pub fn put(&mut self, now: Instant, result: SyncSecretResult) {
        self.entries
            .retain(|_, (at, _)| now.saturating_duration_since(*at) < DEDUPE_WINDOW);
        self.entries.insert(result.secret_id.clone(), (now, result));
    }
}

fn skipped(secret_id: &str, provider_id: &str) -> SyncSecretResult {
    SyncSecretResult {
        secret_id: secret_id.to_owned(),
        provider_id: provider_id.to_owned(),
        models_sync_status: "skipped".to_owned(),
        validation_status: None,
        count: 0,
        added: Vec::new(),
        removed: Vec::new(),
        added_count: 0,
        removed_count: 0,
    }
}

fn pending_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("mcp-pending"))
}

fn result_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("mcp-results"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSyncView {
    pub request_id: String,
    pub secret_count: Option<usize>,
    pub created_at: String,
}

/// Queued sync requests for the "waiting for unlock" banner; needs no vault access.
#[tauri::command]
pub fn mcp_pending_sync_requests(app: AppHandle) -> Result<Vec<PendingSyncView>, String> {
    Ok(read_pending(&pending_directory(&app)?)
        .into_iter()
        .map(|request| PendingSyncView {
            request_id: request.request_id,
            secret_count: request.secret_ids.map(|ids| ids.len()),
            created_at: request.created_at,
        })
        .collect())
}

/// True while the vault is unlocked and the idle timer has not expired; background work must
/// not keep a vault usable past the auto-lock deadline.
fn ready(state: &State<'_, AppState>) -> bool {
    let unlocked = state
        .vault
        .lock()
        .map(|guard| guard.as_ref().is_some_and(|vault| vault.is_unlocked()))
        .unwrap_or(false);
    let expired = state
        .policy
        .lock()
        .map(|policy| policy.is_expired(crate::now_seconds()))
        .unwrap_or(true);
    unlocked && !expired
}

/// Background loop: runs queued sync requests whenever the vault is usable.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut recent = RecentResults::default();
        let mut last_request: Option<Instant> = None;
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            run_once(&app, &mut recent, &mut last_request).await;
        }
    });
}

async fn run_once(app: &AppHandle, recent: &mut RecentResults, last_request: &mut Option<Instant>) {
    let (Ok(pending), Ok(results)) = (pending_directory(app), result_directory(app)) else {
        return;
    };
    let requests = read_pending(&pending);
    if requests.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    for request in requests {
        if !ready(&state) {
            return; // stays pending until the next unlock
        }
        let Some(secrets) = run_request(&request, &state, recent, last_request).await else {
            return;
        };
        let result = SyncResult {
            request_id: &request.request_id,
            status: "done",
            secrets: &secrets,
            finished_at: chrono::Utc::now().to_rfc3339(),
        };
        if queue::write_result(&results, &request.request_id, PREFIX, &result).is_ok() {
            let _ = fs::remove_file(pending.join(format!("{}.json", request.request_id)));
            audit(&state, &request.request_id, secrets.len());
        }
    }
}

fn audit(state: &State<'_, AppState>, request_id: &str, secrets: usize) {
    if let Ok(guard) = state.vault.lock() {
        if let Some(vault) = guard.as_ref() {
            let _ = vault.record_action(
                "model_sync_requested",
                None,
                &serde_json::json!({"source": "mcp", "requestId": request_id, "secrets": secrets})
                    .to_string(),
            );
        }
    }
}

/// `(secret_id, provider_id)` of the keys a request covers; ineligible ids come back as `None` provider.
fn targets(
    request: &PendingSync,
    state: &State<'_, AppState>,
) -> Result<Vec<(String, Option<String>)>, String> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| "vault state is unavailable".to_owned())?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
    let eligible = |item: &secrethub_storage::SecretMetadata| {
        item.value_type == "api_key"
            && !item.archived
            && vault.has_secret_value(&item.id).unwrap_or(false)
    };
    Ok(match &request.secret_ids {
        None => metadata
            .iter()
            .filter(|item| eligible(item))
            .map(|item| (item.id.clone(), Some(item.provider_id.clone())))
            .collect(),
        Some(ids) => {
            let mut seen = BTreeSet::new();
            ids.iter()
                .filter(|id| seen.insert((*id).clone()))
                .map(|id| {
                    let provider = metadata
                        .iter()
                        .find(|item| item.id == *id && eligible(item))
                        .map(|item| item.provider_id.clone());
                    (id.clone(), provider)
                })
                .collect()
        }
    })
}

fn model_ids(state: &State<'_, AppState>, secret_id: &str) -> Vec<String> {
    state
        .vault
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .and_then(|vault| vault.list_provider_models(secret_id).ok())
        })
        .map(|rows| rows.into_iter().map(|row| row.model_id).collect())
        .unwrap_or_default()
}

/// Runs one request; `None` means the vault locked mid-way and the request stays pending.
async fn run_request(
    request: &PendingSync,
    state: &State<'_, AppState>,
    recent: &mut RecentResults,
    last_request: &mut Option<Instant>,
) -> Option<Vec<SyncSecretResult>> {
    let targets = targets(request, state).ok()?;
    let mut results = Vec::with_capacity(targets.len());
    for (secret_id, provider) in targets {
        let Some(provider_id) = provider else {
            results.push(skipped(&secret_id, ""));
            continue;
        };
        if let Some(previous) = recent.get(&secret_id, Instant::now()) {
            results.push(previous.clone());
            continue;
        }
        if let Some(last) = *last_request {
            if let Some(wait) = KEY_SPACING.checked_sub(last.elapsed()) {
                tokio::time::sleep(wait).await;
            }
        }
        let before = model_ids(state, &secret_id);
        let outcome = commands::check_secret(&secret_id, state, true).await;
        *last_request = Some(Instant::now());
        let result = match outcome {
            Ok(report) => {
                let status = match report.models_status.as_str() {
                    "" => "unchanged".to_owned(),
                    other => other.to_owned(),
                };
                let (added, removed) = if status == "ok" {
                    diff_models(&before, &model_ids(state, &secret_id))
                } else {
                    (Vec::new(), Vec::new())
                };
                let (added_count, removed_count) = (added.len(), removed.len());
                SyncSecretResult {
                    secret_id: secret_id.clone(),
                    provider_id,
                    models_sync_status: status,
                    validation_status: report.validation,
                    count: report.count,
                    added: added.into_iter().take(MAX_LISTED_MODELS).collect(),
                    removed: removed.into_iter().take(MAX_LISTED_MODELS).collect(),
                    added_count,
                    removed_count,
                }
            }
            Err(message) if message.contains("locked") => return None,
            Err(_) => SyncSecretResult {
                models_sync_status: "failed".to_owned(),
                ..skipped(&secret_id, &provider_id)
            },
        };
        recent.put(Instant::now(), result.clone());
        results.push(result);
    }
    Some(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_file(id: &str, extra: &str) -> String {
        format!(
            r#"{{"version":1,"requestId":"{id}","kind":"sync_models",{extra}"createdAt":"2026-10-28T00:00:00Z"}}"#
        )
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn diffs_model_sets_into_sorted_added_and_removed() {
        let (added, removed) = diff_models(&ids(&["a", "b", "c"]), &ids(&["b", "d", "c", "z"]));
        assert_eq!(added, ids(&["d", "z"]));
        assert_eq!(removed, ids(&["a"]));
        assert_eq!(diff_models(&[], &ids(&["x"])).0, ids(&["x"]));
        assert_eq!(diff_models(&ids(&["x"]), &ids(&["x"])), (vec![], vec![]));
    }

    #[test]
    fn reuses_a_result_for_sixty_seconds_only() {
        let mut recent = RecentResults::default();
        let start = Instant::now();
        recent.put(start, skipped("secret-1", "deepseek"));
        assert!(recent
            .get("secret-1", start + Duration::from_secs(59))
            .is_some());
        assert!(recent
            .get("secret-1", start + Duration::from_secs(61))
            .is_none());
        assert!(recent.get("secret-2", start).is_none());
    }

    #[test]
    fn reads_only_valid_sync_requests_and_other_readers_ignore_them() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: String| fs::write(dir.path().join(name), body).unwrap();
        write("sync-ok.json", request_file("sync-ok", ""));
        write(
            "sync-ids.json",
            request_file("sync-ids", r#""secretIds":["secret-1"],"#),
        );
        write(
            "sync-bad-kind.json",
            request_file("sync-bad-kind", "").replace("sync_models", "other"),
        );
        write("sync-mismatch.json", request_file("sync-other", ""));
        write(
            "sync-empty.json",
            request_file("sync-empty", r#""secretIds":[],"#),
        );
        write("sync-garbage.json", "{not json".to_owned());
        write(
            "plan-1.json",
            r#"{"requestId":"plan-1","kind":"prepare_project_env","projectPath":"C:/x","secretIds":["s"],"createdAt":"2026-10-28T00:00:00Z"}"#.to_owned(),
        );
        let found = read_pending(dir.path());
        let mut names = found
            .iter()
            .map(|item| item.request_id.as_str())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["sync-ids", "sync-ok"]);
        // Plan and capability readers never see sync files.
        assert!(crate::caps::read_pending(dir.path()).is_empty());
        assert!(!crate::plan::valid_request_id("sync-ok"));
    }

    #[test]
    fn result_file_carries_model_ids_and_counts_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut entry = skipped("secret-1", "deepseek");
        entry.models_sync_status = "ok".into();
        entry.added = ids(&["m2"]);
        entry.added_count = 1;
        let secrets = [entry];
        let result = SyncResult {
            request_id: "sync-r1",
            status: "done",
            secrets: &secrets,
            finished_at: "now".into(),
        };
        queue::write_result(dir.path(), "sync-r1", PREFIX, &result).unwrap();
        let text = fs::read_to_string(dir.path().join("sync-r1.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["status"], "done");
        assert_eq!(json["secrets"][0]["added"][0], "m2");
        assert_eq!(json["secrets"][0]["addedCount"], 1);
        assert!(json["secrets"][0].get("validationStatus").is_none());
    }
}
