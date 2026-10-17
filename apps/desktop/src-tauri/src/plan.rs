//! MCP plan handling: binding expansion (plan v2), value resolution, project `.env`
//! writing and result files. Plan files and results never contain secret values;
//! API keys are decrypted only inside `resolve_entries`, right before writing.

use crate::commands::{selected_entries, selected_entries_from_metadata};
use secrethub_core::{VaultError, VaultService};
use secrethub_exporter::{
    detect_conflicts, ensure_gitignore_env, env_is_tracked, gitignore_has_env, merge_env,
    merge_example, render_env, render_example, validate_export_directory, write_atomic, EnvEntry,
    ExportError,
};
use secrethub_providers::default_registry;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::SystemTime,
};

/// Same vocabulary as the MCP server and design §4.
pub const CAPABILITIES: [&str; 10] = [
    "chat",
    "code",
    "reasoning",
    "vision",
    "image_gen",
    "video_gen",
    "tts",
    "asr",
    "embedding",
    "rerank",
];
pub const MAX_BINDINGS: usize = 50;
pub const MASK: &str = "••••••••";
pub const RESULT_RETENTION: usize = 50;
const MAX_MODEL_ID_LEN: usize = 200;

pub const WARN_MODEL_NOT_IN_LIST: &str = "model_not_in_synced_list";
pub const WARN_MODELS_NOT_SYNCED: &str = "models_not_synced";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanError {
    pub code: String,
    pub message: String,
}

impl PlanError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// Maps exporter failures to stable codes. Parse errors can quote the offending
/// line of an existing `.env`, so their detail is deliberately dropped.
fn export_failure(error: ExportError) -> PlanError {
    match error {
        ExportError::Parse(_) => PlanError::new(
            "parse_error",
            "existing .env or .env.example could not be parsed; fix its syntax first",
        ),
        ExportError::InvalidKey(key) => PlanError::new(
            "invalid_plan",
            format!("invalid environment variable name {key}"),
        ),
        ExportError::InvalidJson(_) => PlanError::new("invalid_plan", "invalid plan data"),
        ExportError::Io(error) => PlanError::new("io", format!("filesystem error: {error}")),
    }
}

fn vault_failure(error: VaultError) -> PlanError {
    match error {
        VaultError::Locked => PlanError::new("vault_locked", "vault is locked"),
        _ => PlanError::new("secret_unavailable", "secret is unavailable"),
    }
}

// ---------------------------------------------------------------- plan model

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanEnv {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanBinding {
    #[serde(alias = "secretId")]
    pub secret_id: String,
    #[serde(default)]
    pub capability: String,
    #[serde(default, alias = "modelId")]
    pub model_id: String,
    #[serde(default)]
    pub env: PlanEnv,
}

/// Accepts both v1 (`secret_ids` only) and v2 (`bindings`) files; the MCP server
/// writes camelCase top-level names, the frontend receives snake_case.
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingPlan {
    #[serde(alias = "requestId")]
    pub request_id: String,
    pub kind: String,
    #[serde(alias = "projectPath")]
    pub project_path: String,
    #[serde(default, alias = "secretIds")]
    pub secret_ids: Vec<String>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub bindings: Vec<PlanBinding>,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

impl PendingPlan {
    fn effective_version(&self) -> u32 {
        self.version.max(1)
    }

    fn validate(&self) -> Result<(), PlanError> {
        if self.project_path.trim().is_empty() {
            return Err(PlanError::new("invalid_plan", "plan has no project path"));
        }
        if self.effective_version() > 2 {
            return Err(PlanError::new("invalid_plan", "unsupported plan version"));
        }
        if self.secret_ids.is_empty() && self.bindings.is_empty() {
            return Err(PlanError::new("invalid_plan", "plan selects nothing"));
        }
        if self.effective_version() == 1 && !self.bindings.is_empty() {
            return Err(PlanError::new(
                "invalid_plan",
                "v1 plan cannot carry bindings",
            ));
        }
        Ok(())
    }
}

/// Metadata of one secret the plan references; no value.
#[derive(Debug, Clone)]
pub struct SecretFacts {
    pub provider_id: String,
    pub endpoint_url: String,
    pub archived: bool,
    pub has_value: bool,
    /// The key's model list was synced and is non-empty.
    pub models_synced: bool,
    pub model_ids: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    ApiKey { secret_id: String },
    BaseUrl { value: String },
    Model { value: String, capability: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEntry {
    pub key: String,
    /// The secret the first binding for this variable came from (metadata, for review only).
    pub secret_id: String,
    pub source: Source,
    pub warning: Option<String>,
}

pub fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn invalid_name(name: &str) -> PlanError {
    PlanError::new(
        "invalid_plan",
        format!("invalid environment variable name {name:?}"),
    )
}

/// Design §9.1. `catalog_base_urls` maps provider id to its default base URL.
pub fn expand_bindings(
    bindings: &[PlanBinding],
    secrets: &HashMap<String, SecretFacts>,
    catalog_base_urls: &HashMap<String, String>,
) -> Result<Vec<PlannedEntry>, PlanError> {
    if bindings.is_empty() || bindings.len() > MAX_BINDINGS {
        return Err(PlanError::new(
            "invalid_plan",
            format!("a plan needs 1 to {MAX_BINDINGS} bindings"),
        ));
    }
    let mut planned: Vec<PlannedEntry> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut add = |key: &str,
                   secret_id: &str,
                   source: Source,
                   warning: Option<String>|
     -> Result<(), PlanError> {
        match by_key.get(key) {
            Some(&index) if planned[index].source == source => Ok(()),
            Some(_) => Err(PlanError::new(
                "env_conflict",
                format!("environment variable {key} is bound to different values"),
            )),
            None => {
                by_key.insert(key.to_owned(), planned.len());
                planned.push(PlannedEntry {
                    key: key.to_owned(),
                    secret_id: secret_id.to_owned(),
                    source,
                    warning,
                });
                Ok(())
            }
        }
    };
    for binding in bindings {
        let env = &binding.env;
        if env.api_key.is_none() && env.base_url.is_none() && env.model.is_none() {
            return Err(PlanError::new(
                "invalid_plan",
                "a binding needs at least one env name",
            ));
        }
        for name in [&env.api_key, &env.base_url, &env.model]
            .into_iter()
            .flatten()
        {
            if !valid_env_name(name) {
                return Err(invalid_name(name));
            }
        }
        if !CAPABILITIES.contains(&binding.capability.as_str()) {
            return Err(PlanError::new("invalid_plan", "unknown capability"));
        }
        let facts = secrets
            .get(&binding.secret_id)
            .ok_or_else(|| PlanError::new("secret_unavailable", "secret not found"))?;
        if facts.archived || !facts.has_value {
            return Err(PlanError::new(
                "secret_unavailable",
                "secret is archived or has no value",
            ));
        }
        if let Some(name) = &env.api_key {
            add(
                name,
                &binding.secret_id,
                Source::ApiKey {
                    secret_id: binding.secret_id.clone(),
                },
                None,
            )?;
        }
        if let Some(name) = &env.base_url {
            let from_secret = facts.endpoint_url.trim();
            let value = if from_secret.is_empty() {
                catalog_base_urls
                    .get(&facts.provider_id)
                    .map(|url| url.trim())
                    .unwrap_or_default()
            } else {
                from_secret
            };
            if value.is_empty() {
                return Err(PlanError::new(
                    "invalid_plan",
                    format!("no base URL is known for the secret bound to {name}"),
                ));
            }
            add(
                name,
                &binding.secret_id,
                Source::BaseUrl {
                    value: value.to_owned(),
                },
                None,
            )?;
        }
        if let Some(name) = &env.model {
            let model_id = binding.model_id.trim();
            if model_id.is_empty()
                || model_id.chars().count() > MAX_MODEL_ID_LEN
                || model_id.chars().any(char::is_control)
            {
                return Err(PlanError::new(
                    "invalid_plan",
                    format!("binding for {name} needs a valid model_id"),
                ));
            }
            let warning = if !facts.models_synced {
                Some(WARN_MODELS_NOT_SYNCED.to_owned())
            } else if !facts.model_ids.contains(model_id) {
                Some(WARN_MODEL_NOT_IN_LIST.to_owned())
            } else {
                None
            };
            add(
                name,
                &binding.secret_id,
                Source::Model {
                    value: model_id.to_owned(),
                    capability: binding.capability.clone(),
                },
                warning,
            )?;
        }
    }
    Ok(planned)
}

pub struct Resolved {
    pub entries: Vec<EnvEntry>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

/// Turns planned entries into env entries. `read_secret` is only called for
/// API-key entries that are not skipped.
pub fn resolve_entries(
    planned: &[PlannedEntry],
    skip: &HashSet<String>,
    read_secret: &mut dyn FnMut(&str) -> Result<String, PlanError>,
) -> Result<Resolved, PlanError> {
    let mut resolved = Resolved {
        entries: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
    };
    for entry in planned {
        if skip.contains(&entry.key) {
            resolved.skipped.push(entry.key.clone());
            continue;
        }
        let value = match &entry.source {
            Source::ApiKey { secret_id } => read_secret(secret_id)?,
            Source::BaseUrl { value } | Source::Model { value, .. } => value.clone(),
        };
        if let Some(warning) = &entry.warning {
            resolved.warnings.push(format!("{}: {warning}", entry.key));
        }
        resolved.entries.push(EnvEntry::new(&entry.key, value));
    }
    Ok(resolved)
}

// ------------------------------------------------------------- env writing

pub struct WriteOptions {
    pub ensure_gitignore: bool,
    pub write_example: bool,
    pub merge_existing: bool,
    pub replace_existing: bool,
}

fn read_optional(path: &Path) -> Result<String, PlanError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(PlanError::new("io", format!("filesystem error: {error}"))),
    }
}

/// Shared by `export_env` and plan confirmation: tracked-.env and .gitignore
/// checks run before anything is written.
pub fn write_project_env(
    directory: &Path,
    entries: &[EnvEntry],
    options: &WriteOptions,
) -> Result<(), PlanError> {
    let directory = validate_export_directory(directory).map_err(export_failure)?;
    if env_is_tracked(&directory).map_err(export_failure)? {
        return Err(PlanError::new(
            "env_tracked",
            ".env is already tracked by Git; remediation required before export",
        ));
    }
    let gitignore = directory.join(".gitignore");
    if !gitignore_has_env(&gitignore).map_err(export_failure)? {
        if options.ensure_gitignore {
            ensure_gitignore_env(&gitignore).map_err(export_failure)?;
        } else {
            return Err(PlanError::new(
                "gitignore",
                ".env is not protected by .gitignore; explicit remediation required",
            ));
        }
    }
    let env_path = directory.join(".env");
    let example_path = directory.join(".env.example");
    if options.merge_existing {
        let content = merge_env(&read_optional(&env_path)?, entries).map_err(export_failure)?;
        let example = if options.write_example {
            Some(merge_example(&read_optional(&example_path)?, entries).map_err(export_failure)?)
        } else {
            None
        };
        write_atomic(&env_path, &content, true).map_err(export_failure)?;
        if let Some(example) = example {
            write_atomic(&example_path, &example, true).map_err(export_failure)?;
        }
    } else {
        let content = render_env(entries).map_err(export_failure)?;
        write_atomic(&env_path, &content, options.replace_existing).map_err(export_failure)?;
        if options.write_example {
            let example = render_example(entries).map_err(export_failure)?;
            write_atomic(&example_path, &example, options.replace_existing)
                .map_err(export_failure)?;
        }
    }
    Ok(())
}

// ------------------------------------------------------- vault integration

fn catalog_base_urls() -> HashMap<String, String> {
    default_registry()
        .into_iter()
        .map(|provider| (provider.id, provider.base_url))
        .collect()
}

fn build_facts(
    vault: &VaultService,
    metadata: &[secrethub_storage::SecretMetadata],
    ids: &HashSet<&str>,
) -> Result<HashMap<String, SecretFacts>, PlanError> {
    let mut facts = HashMap::new();
    for item in metadata
        .iter()
        .filter(|item| ids.contains(item.id.as_str()))
    {
        let model_ids = vault
            .list_provider_models(&item.id)
            .map_err(vault_failure)?
            .into_iter()
            .map(|row| row.model_id)
            .collect::<HashSet<_>>();
        facts.insert(
            item.id.clone(),
            SecretFacts {
                provider_id: item.provider_id.clone(),
                endpoint_url: item.endpoint_url.clone(),
                archived: item.archived,
                has_value: vault.has_secret_value(&item.id).map_err(vault_failure)?,
                models_synced: item.models_synced_at > 0 && !model_ids.is_empty(),
                model_ids,
            },
        );
    }
    Ok(facts)
}

fn expand_for_plan(
    plan: &PendingPlan,
    vault: &VaultService,
    metadata: &[secrethub_storage::SecretMetadata],
) -> Result<Vec<PlannedEntry>, PlanError> {
    if plan.bindings.is_empty() {
        return Ok(Vec::new());
    }
    let ids = plan
        .bindings
        .iter()
        .map(|binding| binding.secret_id.as_str())
        .collect::<HashSet<_>>();
    expand_bindings(
        &plan.bindings,
        &build_facts(vault, metadata, &ids)?,
        &catalog_base_urls(),
    )
}

/// v1 `(key, secret_id)` pairs of the plan from metadata only (no decryption); a missing
/// secret is an error.
fn legacy_keys(
    plan: &PendingPlan,
    metadata: &[secrethub_storage::SecretMetadata],
) -> Result<Vec<(String, String)>, PlanError> {
    let mut keys = Vec::new();
    for id in &plan.secret_ids {
        let item = metadata
            .iter()
            .find(|item| item.id == *id)
            .ok_or_else(|| PlanError::new("secret_unavailable", "secret not found"))?;
        keys.extend(
            selected_entries_from_metadata(item, String::new())
                .into_iter()
                .map(|entry| (entry.key, item.id.clone())),
        );
    }
    Ok(keys)
}

fn reject_overlap(legacy: &[(String, String)], planned: &[PlannedEntry]) -> Result<(), PlanError> {
    match planned
        .iter()
        .find(|entry| legacy.iter().any(|(key, _)| *key == entry.key))
    {
        Some(entry) => Err(PlanError::new(
            "env_conflict",
            format!(
                "environment variable {} is produced by both secret_ids and bindings",
                entry.key
            ),
        )),
        None => Ok(()),
    }
}

pub struct Outcome {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

/// Validates, resolves and writes one confirmed plan. Never returns values.
pub fn process_plan(
    plan: &PendingPlan,
    skip: &HashSet<String>,
    vault: &VaultService,
) -> Result<Outcome, PlanError> {
    plan.validate()?;
    let metadata = vault.list_metadata().map_err(vault_failure)?;
    let planned = expand_for_plan(plan, vault, &metadata)?;
    let legacy_names = legacy_keys(plan, &metadata)?;
    reject_overlap(&legacy_names, &planned)?;

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    if !plan.secret_ids.is_empty() {
        for entry in selected_entries(vault, &plan.secret_ids)
            .map_err(|_| PlanError::new("secret_unavailable", "secret is unavailable"))?
        {
            if skip.contains(&entry.key) {
                skipped.push(entry.key);
            } else {
                entries.push(entry);
            }
        }
    }
    let resolved = resolve_entries(&planned, skip, &mut |id| {
        vault.read_secret(id).map_err(vault_failure)
    })?;
    skipped.extend(resolved.skipped);
    entries.extend(resolved.entries);

    if !entries.is_empty() {
        write_project_env(
            Path::new(&plan.project_path),
            &entries,
            &WriteOptions {
                ensure_gitignore: true,
                write_example: true,
                merge_existing: true,
                replace_existing: false,
            },
        )?;
        let mut audited = plan.secret_ids.clone();
        for entry in &planned {
            if let Source::ApiKey { secret_id } = &entry.source {
                audited.push(secret_id.clone());
            }
        }
        audited.sort();
        audited.dedup();
        for id in audited {
            // The files are already written; an audit failure must not turn this into a failure.
            let _ = vault.record_action("env_exported", Some(&id), "{\"source\":\"desktop\"}");
        }
    }
    Ok(Outcome {
        written: entries.into_iter().map(|entry| entry.key).collect(),
        skipped,
        warnings: resolved.warnings,
    })
}

// ------------------------------------------------------------------ preview

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEntry {
    pub key: String,
    /// Name of the secret this entry comes from (metadata); untrusted text for display only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// `api_key` | `base_url` | `model` | `v1`
    pub kind: &'static str,
    /// Masked for API keys and v1 entries; plain for base URLs and model ids.
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// The key already exists in the project's `.env`.
    pub conflict: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanPreview {
    pub request_id: String,
    pub project_path: String,
    pub version: u32,
    pub entries: Vec<PreviewEntry>,
    pub errors: Vec<PlanError>,
}

pub fn preview_entries(
    legacy: &[(String, String)],
    planned: &[PlannedEntry],
    existing_env: &str,
    secret_names: &HashMap<String, String>,
) -> Result<Vec<PreviewEntry>, PlanError> {
    let mut entries = legacy
        .iter()
        .map(|(key, secret_id)| PreviewEntry {
            key: key.clone(),
            secret_name: secret_names.get(secret_id).cloned(),
            kind: "v1",
            display: MASK.to_owned(),
            capability: None,
            conflict: false,
            warnings: Vec::new(),
        })
        .collect::<Vec<_>>();
    for entry in planned {
        let (kind, display, capability) = match &entry.source {
            Source::ApiKey { .. } => ("api_key", MASK.to_owned(), None),
            Source::BaseUrl { value } => ("base_url", value.clone(), None),
            Source::Model { value, capability } => {
                ("model", value.clone(), Some(capability.clone()))
            }
        };
        entries.push(PreviewEntry {
            key: entry.key.clone(),
            secret_name: secret_names.get(&entry.secret_id).cloned(),
            kind,
            display,
            capability,
            conflict: false,
            warnings: entry.warning.iter().cloned().collect(),
        });
    }
    let probe = entries
        .iter()
        .map(|entry| EnvEntry::new(&entry.key, ""))
        .collect::<Vec<_>>();
    let conflicts = detect_conflicts(existing_env, &probe)
        .map_err(export_failure)?
        .into_iter()
        .collect::<HashSet<_>>();
    for entry in &mut entries {
        entry.conflict = conflicts.contains(&entry.key);
    }
    Ok(entries)
}

/// Read-only: decrypts nothing and returns no value for any API key.
pub fn preview_plan(plan: &PendingPlan, vault: &VaultService) -> PlanPreview {
    let build = || -> Result<Vec<PreviewEntry>, PlanError> {
        plan.validate()?;
        let metadata = vault.list_metadata().map_err(vault_failure)?;
        let planned = expand_for_plan(plan, vault, &metadata)?;
        let legacy = legacy_keys(plan, &metadata)?;
        reject_overlap(&legacy, &planned)?;
        let existing = read_optional(&Path::new(&plan.project_path).join(".env"))?;
        let names = metadata
            .iter()
            .map(|item| (item.id.clone(), item.name.clone()))
            .collect::<HashMap<_, _>>();
        preview_entries(&legacy, &planned, &existing, &names)
    };
    let (entries, errors) = match build() {
        Ok(entries) => (entries, Vec::new()),
        Err(error) => (Vec::new(), vec![error]),
    };
    PlanPreview {
        request_id: plan.request_id.clone(),
        project_path: plan.project_path.clone(),
        version: plan.effective_version(),
        entries,
        errors,
    }
}

// ------------------------------------------------------------ result files

pub fn valid_request_id(id: &str) -> bool {
    id.starts_with("plan-")
        && id.len() <= 128
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanResult {
    pub request_id: String,
    /// `written` | `declined` | `failed`
    pub status: String,
    pub written_keys: Vec<String>,
    pub skipped_keys: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<PlanError>,
    pub finished_at: String,
}

impl PlanResult {
    fn base(request_id: &str, status: &str) -> Self {
        Self {
            request_id: request_id.to_owned(),
            status: status.to_owned(),
            written_keys: Vec::new(),
            skipped_keys: Vec::new(),
            warnings: Vec::new(),
            error: None,
            finished_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn written(request_id: &str, outcome: Outcome) -> Self {
        Self {
            written_keys: outcome.written,
            skipped_keys: outcome.skipped,
            warnings: outcome.warnings,
            ..Self::base(request_id, "written")
        }
    }

    pub fn declined(request_id: &str) -> Self {
        Self::base(request_id, "declined")
    }

    pub fn failed(request_id: &str, error: &PlanError) -> Self {
        Self {
            error: Some(error.clone()),
            ..Self::base(request_id, "failed")
        }
    }
}

/// Writes `<directory>/<requestId>.json`, then keeps only the newest `RESULT_RETENTION` files.
pub fn write_result(directory: &Path, result: &PlanResult) -> Result<(), PlanError> {
    if !valid_request_id(&result.request_id) {
        return Err(PlanError::new("invalid_plan", "invalid request id"));
    }
    let io_error =
        |error: std::io::Error| PlanError::new("io", format!("filesystem error: {error}"));
    fs::create_dir_all(directory).map_err(io_error)?;
    let json = serde_json::to_string(result)
        .map_err(|_| PlanError::new("io", "result could not be serialized"))?;
    let target = directory.join(format!("{}.json", result.request_id));
    let temporary = directory.join(format!("{}.json.tmp", result.request_id));
    fs::write(&temporary, format!("{json}\n")).map_err(io_error)?;
    fs::rename(&temporary, &target).map_err(io_error)?;
    prune_results(directory, RESULT_RETENTION);
    Ok(())
}

fn prune_results(directory: &Path, keep: usize) {
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    let mut files = read
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, entry.file_name(), entry.path())
        })
        .collect::<Vec<_>>();
    if files.len() <= keep {
        return;
    }
    files.sort();
    for (_, _, path) in files.iter().take(files.len() - keep) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_core::NewSecretInput;
    use secrethub_storage::Database;

    const FAKE_VALUE: &str = "fake-test-value-do-not-leak";

    fn facts(endpoint: &str, models: &[&str]) -> SecretFacts {
        SecretFacts {
            provider_id: "agg".to_owned(),
            endpoint_url: endpoint.to_owned(),
            archived: false,
            has_value: true,
            models_synced: !models.is_empty(),
            model_ids: models.iter().map(|model| (*model).to_owned()).collect(),
        }
    }

    fn binding(secret: &str, capability: &str, model: &str, env: PlanEnv) -> PlanBinding {
        PlanBinding {
            secret_id: secret.to_owned(),
            capability: capability.to_owned(),
            model_id: model.to_owned(),
            env,
        }
    }

    fn env(api_key: Option<&str>, base_url: Option<&str>, model: Option<&str>) -> PlanEnv {
        PlanEnv {
            api_key: api_key.map(str::to_owned),
            base_url: base_url.map(str::to_owned),
            model: model.map(str::to_owned),
        }
    }

    fn secrets(items: Vec<(&str, SecretFacts)>) -> HashMap<String, SecretFacts> {
        items
            .into_iter()
            .map(|(id, facts)| (id.to_owned(), facts))
            .collect()
    }

    fn catalog() -> HashMap<String, String> {
        HashMap::from([("agg".to_owned(), "https://catalog.example/v1".to_owned())])
    }

    #[test]
    fn shared_api_key_is_written_once_and_each_binding_gets_a_model_line() {
        let bindings = vec![
            binding(
                "s1",
                "chat",
                "chat-model",
                env(Some("API_KEY"), Some("BASE_URL"), Some("LLM_MODEL")),
            ),
            binding(
                "s1",
                "image_gen",
                "image-model",
                env(Some("API_KEY"), Some("BASE_URL"), Some("IMAGE_MODEL")),
            ),
        ];
        let secrets = secrets(vec![("s1", facts("", &["chat-model", "image-model"]))]);
        let planned = expand_bindings(&bindings, &secrets, &catalog()).unwrap();
        let keys = planned
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, ["API_KEY", "BASE_URL", "LLM_MODEL", "IMAGE_MODEL"]);
        assert_eq!(
            planned[0].source,
            Source::ApiKey {
                secret_id: "s1".to_owned()
            }
        );
        assert_eq!(
            planned[1].source,
            Source::BaseUrl {
                value: "https://catalog.example/v1".to_owned()
            }
        );
        assert_eq!(
            planned[3].source,
            Source::Model {
                value: "image-model".to_owned(),
                capability: "image_gen".to_owned()
            }
        );
        assert!(planned.iter().all(|entry| entry.warning.is_none()));
    }

    #[test]
    fn same_name_with_different_value_rejects_the_whole_plan() {
        let secrets = secrets(vec![("s1", facts("", &["a", "b"])), ("s2", facts("", &[]))]);
        let model_clash = vec![
            binding("s1", "chat", "a", env(None, None, Some("LLM_MODEL"))),
            binding("s1", "code", "b", env(None, None, Some("LLM_MODEL"))),
        ];
        assert_eq!(
            expand_bindings(&model_clash, &secrets, &catalog())
                .unwrap_err()
                .code,
            "env_conflict"
        );
        let key_clash = vec![
            binding("s1", "chat", "", env(Some("API_KEY"), None, None)),
            binding("s2", "chat", "", env(Some("API_KEY"), None, None)),
        ];
        assert_eq!(
            expand_bindings(&key_clash, &secrets, &catalog())
                .unwrap_err()
                .code,
            "env_conflict"
        );
        let kind_clash = vec![
            binding("s1", "chat", "a", env(Some("X"), None, None)),
            binding("s1", "chat", "a", env(None, None, Some("X"))),
        ];
        assert_eq!(
            expand_bindings(&kind_clash, &secrets, &catalog())
                .unwrap_err()
                .code,
            "env_conflict"
        );
    }

    #[test]
    fn identical_duplicates_collapse() {
        let secrets = secrets(vec![("s1", facts("", &["a"]))]);
        let bindings = vec![
            binding("s1", "chat", "a", env(None, None, Some("LLM_MODEL"))),
            binding("s1", "chat", "a", env(None, None, Some("LLM_MODEL"))),
        ];
        assert_eq!(
            expand_bindings(&bindings, &secrets, &catalog())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn missing_archived_or_valueless_secret_is_rejected() {
        let mut archived = facts("", &[]);
        archived.archived = true;
        let mut empty = facts("", &[]);
        empty.has_value = false;
        let secrets = secrets(vec![("archived", archived), ("empty", empty)]);
        for id in ["missing", "archived", "empty"] {
            let bindings = vec![binding(id, "chat", "", env(Some("K"), None, None))];
            assert_eq!(
                expand_bindings(&bindings, &secrets, &catalog())
                    .unwrap_err()
                    .code,
                "secret_unavailable",
                "{id}"
            );
        }
    }

    #[test]
    fn base_url_prefers_secret_then_catalog_and_rejects_when_unknown() {
        let secrets = secrets(vec![
            ("own", facts("https://own.example", &[])),
            ("cat", facts("", &[])),
        ]);
        let own = expand_bindings(
            &[binding("own", "chat", "", env(None, Some("U"), None))],
            &secrets,
            &catalog(),
        )
        .unwrap();
        assert_eq!(
            own[0].source,
            Source::BaseUrl {
                value: "https://own.example".to_owned()
            }
        );
        let fallback = expand_bindings(
            &[binding("cat", "chat", "", env(None, Some("U"), None))],
            &secrets,
            &catalog(),
        )
        .unwrap();
        assert_eq!(
            fallback[0].source,
            Source::BaseUrl {
                value: "https://catalog.example/v1".to_owned()
            }
        );
        let error = expand_bindings(
            &[binding("cat", "chat", "", env(None, Some("U"), None))],
            &secrets,
            &HashMap::new(),
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_plan");
    }

    #[test]
    fn model_warnings_for_unlisted_and_unsynced() {
        let secrets = secrets(vec![
            ("synced", facts("", &["known"])),
            ("never", facts("", &[])),
        ]);
        let bindings = vec![
            binding("synced", "chat", "known", env(None, None, Some("A"))),
            binding("synced", "code", "unknown", env(None, None, Some("B"))),
            binding("never", "chat", "whatever", env(None, None, Some("C"))),
        ];
        let planned = expand_bindings(&bindings, &secrets, &catalog()).unwrap();
        let warnings = planned
            .iter()
            .map(|entry| entry.warning.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            warnings,
            [
                None,
                Some(WARN_MODEL_NOT_IN_LIST),
                Some(WARN_MODELS_NOT_SYNCED)
            ]
        );
    }

    #[test]
    fn rejects_malformed_bindings() {
        let secrets = secrets(vec![("s1", facts("", &["a"]))]);
        let cases = vec![
            binding("s1", "chat", "a", env(None, None, None)),
            binding("s1", "chat", "a", env(Some("1BAD"), None, None)),
            binding("s1", "chat", "a", env(Some("HAS SPACE"), None, None)),
            binding("s1", "nonsense", "a", env(Some("K"), None, None)),
            binding("s1", "chat", "", env(None, None, Some("M"))),
            binding("s1", "chat", "line\nbreak", env(None, None, Some("M"))),
        ];
        for case in cases {
            let code = expand_bindings(std::slice::from_ref(&case), &secrets, &catalog())
                .unwrap_err()
                .code;
            assert_eq!(code, "invalid_plan", "{case:?}");
        }
        assert!(expand_bindings(&[], &secrets, &catalog()).is_err());
        let many = vec![binding("s1", "chat", "a", env(Some("K"), None, None)); MAX_BINDINGS + 1];
        assert!(expand_bindings(&many, &secrets, &catalog()).is_err());
    }

    #[test]
    fn skip_keys_drop_entries_without_reading_their_secret() {
        let planned = vec![
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "API_KEY".to_owned(),
                source: Source::ApiKey {
                    secret_id: "s1".to_owned(),
                },
                warning: None,
            },
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "LLM_MODEL".to_owned(),
                source: Source::Model {
                    value: "m".to_owned(),
                    capability: "chat".to_owned(),
                },
                warning: Some(WARN_MODEL_NOT_IN_LIST.to_owned()),
            },
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "OTHER_MODEL".to_owned(),
                source: Source::Model {
                    value: "n".to_owned(),
                    capability: "code".to_owned(),
                },
                warning: Some(WARN_MODELS_NOT_SYNCED.to_owned()),
            },
        ];
        let skip = HashSet::from(["API_KEY".to_owned(), "OTHER_MODEL".to_owned()]);
        let mut reads = 0;
        let resolved = resolve_entries(&planned, &skip, &mut |_| {
            reads += 1;
            Ok(FAKE_VALUE.to_owned())
        })
        .unwrap();
        assert_eq!(reads, 0);
        assert_eq!(resolved.skipped, ["API_KEY", "OTHER_MODEL"]);
        assert_eq!(resolved.entries, vec![EnvEntry::new("LLM_MODEL", "m")]);
        assert_eq!(resolved.warnings, ["LLM_MODEL: model_not_in_synced_list"]);
    }

    #[test]
    fn preview_masks_keys_marks_conflicts_and_never_holds_values() {
        let planned = vec![
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "API_KEY".to_owned(),
                source: Source::ApiKey {
                    secret_id: "s1".to_owned(),
                },
                warning: None,
            },
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "BASE_URL".to_owned(),
                source: Source::BaseUrl {
                    value: "https://x.example".to_owned(),
                },
                warning: None,
            },
            PlannedEntry {
                secret_id: "s1".to_owned(),
                key: "LLM_MODEL".to_owned(),
                source: Source::Model {
                    value: "m".to_owned(),
                    capability: "chat".to_owned(),
                },
                warning: Some(WARN_MODEL_NOT_IN_LIST.to_owned()),
            },
        ];
        let entries = preview_entries(
            &[("LEGACY_KEY".to_owned(), "s0".to_owned())],
            &planned,
            "API_KEY=old\nUNRELATED=1\n",
            &HashMap::from([
                ("s0".to_owned(), "Legacy secret".to_owned()),
                ("s1".to_owned(), "Main key".to_owned()),
            ]),
        )
        .unwrap();
        let summary = entries
            .iter()
            .map(|entry| {
                (
                    entry.key.as_str(),
                    entry.kind,
                    entry.display.as_str(),
                    entry.conflict,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                ("LEGACY_KEY", "v1", MASK, false),
                ("API_KEY", "api_key", MASK, true),
                ("BASE_URL", "base_url", "https://x.example", false),
                ("LLM_MODEL", "model", "m", false),
            ]
        );
        assert_eq!(entries[0].secret_name.as_deref(), Some("Legacy secret"));
        assert_eq!(entries[1].secret_name.as_deref(), Some("Main key"));
        assert_eq!(entries[3].capability.as_deref(), Some("chat"));
        assert_eq!(entries[3].warnings, [WARN_MODEL_NOT_IN_LIST]);
        assert!(!serde_json::to_string(&entries).unwrap().contains("old"));
    }

    #[test]
    fn plan_files_parse_in_v1_and_v2_shapes() {
        let v1: PendingPlan = serde_json::from_str(r#"{"requestId":"plan-1","kind":"prepare_project_env","projectPath":"C:/p","secretIds":["a"],"createdAt":"t"}"#).unwrap();
        assert_eq!(
            (
                v1.effective_version(),
                v1.secret_ids.len(),
                v1.bindings.len()
            ),
            (1, 1, 0)
        );
        assert!(v1.validate().is_ok());
        let v2: PendingPlan = serde_json::from_str(r#"{"version":2,"requestId":"plan-2","kind":"prepare_project_env","projectPath":"C:/p","bindings":[{"secret_id":"a","capability":"chat","model_id":"m","env":{"model":"LLM_MODEL"}}],"createdAt":"t"}"#).unwrap();
        assert_eq!(
            (
                v2.effective_version(),
                v2.secret_ids.len(),
                v2.bindings.len()
            ),
            (2, 0, 1)
        );
        assert!(v2.validate().is_ok());
        let empty: PendingPlan = serde_json::from_str(
            r#"{"version":2,"requestId":"plan-3","kind":"k","projectPath":"C:/p","createdAt":"t"}"#,
        )
        .unwrap();
        assert_eq!(empty.validate().unwrap_err().code, "invalid_plan");
    }

    #[test]
    fn request_ids_are_validated() {
        assert!(valid_request_id("plan-1760000000000-abc123"));
        for bad in [
            "",
            "plan",
            "x-plan-1",
            "plan-../a",
            "plan-a/b",
            "plan-a\\b",
            "plan-a.json",
            &format!("plan-{}", "a".repeat(200)),
        ] {
            assert!(!valid_request_id(bad), "{bad}");
        }
    }

    #[test]
    fn result_files_are_written_and_only_the_newest_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let base = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for index in 0..55u64 {
            let id = format!("plan-{index:04}");
            write_result(directory.path(), &PlanResult::declined(&id)).unwrap();
            let file = fs::File::options()
                .write(true)
                .open(directory.path().join(format!("{id}.json")))
                .unwrap();
            file.set_modified(base + std::time::Duration::from_secs(index))
                .unwrap();
        }
        write_result(
            directory.path(),
            &PlanResult::failed("plan-final", &PlanError::new("env_tracked", "msg")),
        )
        .unwrap();
        let names = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(names.len(), RESULT_RETENTION);
        assert!(names.contains("plan-final.json"));
        assert!(!names.contains("plan-0000.json") && !names.contains("plan-0005.json"));
        assert!(names.contains("plan-0006.json"));
        let text = fs::read_to_string(directory.path().join("plan-final.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["status"], "failed");
        assert_eq!(value["error"]["code"], "env_tracked");
        assert!(value["finishedAt"].is_string());
        assert!(write_result(directory.path(), &PlanResult::declined("plan-../evil")).is_err());
    }

    fn vault_with_key(directory: &Path) -> (VaultService, String) {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        let id = vault
            .create_secret(NewSecretInput {
                name: "Test key".to_owned(),
                provider_id: "openrouter".to_owned(),
                env_key: "OPENROUTER_API_KEY".to_owned(),
                description: String::new(),
                tags: Vec::new(),
                value: FAKE_VALUE.to_owned(),
            })
            .unwrap();
        (vault, id)
    }

    #[test]
    fn process_plan_writes_env_once_and_leaves_no_value_in_results() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let (vault, id) = vault_with_key(data.path());
        fs::write(project.path().join(".env"), "KEEP=1\nLLM_MODEL=old\n").unwrap();
        let plan = PendingPlan {
            request_id: "plan-1-aa".to_owned(),
            kind: "prepare_project_env".to_owned(),
            project_path: project.path().to_string_lossy().into_owned(),
            secret_ids: Vec::new(),
            version: 2,
            bindings: vec![
                binding(
                    &id,
                    "chat",
                    "m-chat",
                    env(Some("API_KEY"), Some("BASE_URL"), Some("LLM_MODEL")),
                ),
                binding(
                    &id,
                    "image_gen",
                    "m-img",
                    env(Some("API_KEY"), None, Some("IMAGE_MODEL")),
                ),
            ],
            created_at: "t".to_owned(),
        };
        let preview = preview_plan(&plan, &vault);
        assert!(preview.errors.is_empty(), "{:?}", preview.errors);
        assert_eq!(
            preview
                .entries
                .iter()
                .filter(|entry| entry.conflict)
                .count(),
            1
        );
        assert!(!serde_json::to_string(&preview)
            .unwrap()
            .contains(FAKE_VALUE));

        let skip = HashSet::from(["IMAGE_MODEL".to_owned()]);
        let outcome = process_plan(&plan, &skip, &vault).unwrap();
        assert_eq!(outcome.written, ["API_KEY", "BASE_URL", "LLM_MODEL"]);
        assert_eq!(outcome.skipped, ["IMAGE_MODEL"]);
        assert_eq!(outcome.warnings, ["LLM_MODEL: models_not_synced"]);
        let env_text = fs::read_to_string(project.path().join(".env")).unwrap();
        assert!(env_text.contains("KEEP=1"));
        assert_eq!(env_text.matches("API_KEY=").count(), 1);
        assert!(env_text.contains(&format!("API_KEY=\"{FAKE_VALUE}\"")));
        assert!(env_text.contains("LLM_MODEL=\"m-chat\"") && !env_text.contains("LLM_MODEL=old"));
        assert!(!env_text.contains("IMAGE_MODEL"));
        let example = fs::read_to_string(project.path().join(".env.example")).unwrap();
        assert!(!example.contains(FAKE_VALUE));
        assert!(fs::read_to_string(project.path().join(".gitignore"))
            .unwrap()
            .contains(".env"));

        let results = data.path().join("mcp-results");
        write_result(&results, &PlanResult::written(&plan.request_id, outcome)).unwrap();
        let stored = fs::read_to_string(results.join("plan-1-aa.json")).unwrap();
        assert!(!stored.contains(FAKE_VALUE));
        assert!(stored.contains("\"writtenKeys\":[\"API_KEY\",\"BASE_URL\",\"LLM_MODEL\"]"));
    }

    #[test]
    fn process_plan_reports_stable_error_codes() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let (vault, id) = vault_with_key(data.path());
        let mut plan = PendingPlan {
            request_id: "plan-2-bb".to_owned(),
            kind: "prepare_project_env".to_owned(),
            project_path: project.path().to_string_lossy().into_owned(),
            secret_ids: Vec::new(),
            version: 2,
            bindings: vec![binding("nope", "chat", "", env(Some("K"), None, None))],
            created_at: "t".to_owned(),
        };
        assert_eq!(
            process_plan(&plan, &HashSet::new(), &vault)
                .err()
                .unwrap()
                .code,
            "secret_unavailable"
        );
        plan.bindings = vec![binding(&id, "chat", "", env(Some("K"), None, None))];
        fs::write(project.path().join(".env"), "BROKEN LINE WITHOUT EQUALS\n").unwrap();
        let error = process_plan(&plan, &HashSet::new(), &vault).err().unwrap();
        assert_eq!(error.code, "parse_error");
        assert!(!error.message.contains("BROKEN"));
        plan.project_path = project
            .path()
            .join("missing")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            process_plan(&plan, &HashSet::new(), &vault)
                .err()
                .unwrap()
                .code,
            "io"
        );
        let mut vault = vault;
        vault.lock();
        assert_eq!(
            process_plan(&plan, &HashSet::new(), &vault)
                .err()
                .unwrap()
                .code,
            "vault_locked"
        );
    }

    #[test]
    fn v1_plans_keep_using_the_legacy_entries() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let (vault, id) = vault_with_key(data.path());
        let plan = PendingPlan {
            request_id: "plan-3-cc".to_owned(),
            kind: "prepare_project_env".to_owned(),
            project_path: project.path().to_string_lossy().into_owned(),
            secret_ids: vec![id],
            version: 0,
            bindings: Vec::new(),
            created_at: "t".to_owned(),
        };
        let preview = preview_plan(&plan, &vault);
        assert_eq!(preview.version, 1);
        assert_eq!(
            preview
                .entries
                .iter()
                .map(|entry| (entry.key.as_str(), entry.kind))
                .collect::<Vec<_>>(),
            [("OPENROUTER_API_KEY", "v1")]
        );
        let outcome =
            process_plan(&plan, &HashSet::from(["NOT_THERE".to_owned()]), &vault).unwrap();
        assert_eq!(outcome.written, ["OPENROUTER_API_KEY"]);
        assert!(fs::read_to_string(project.path().join(".env"))
            .unwrap()
            .contains(FAKE_VALUE));
    }
}
