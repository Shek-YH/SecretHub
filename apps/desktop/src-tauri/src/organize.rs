//! AI-organized import (design 2026-11-10 sections 1-3). Three MCP queue kinds live here:
//!
//! * `inspect-*.json` (paths only): Desktop scans the files, builds the masked view and keeps the
//!   placeholder mapping in memory for 30 minutes. Needs no unlocked vault.
//! * `organize-*.json` (the AI's plan, referring to placeholders): written into the vault as soon
//!   as it is unlocked, without a dialog, under the guard rails of section 3 - secrets stay
//!   secret, nothing existing is changed or deleted, every write belongs to a batch that the
//!   user can undo.
//!
//! Real values exist only inside this process. Result files and the audit trail carry names,
//! counts and reason codes, never values.

use crate::import::PlannedSecret;
use crate::import_flow::{same_name, snapshot, unique_name, Snapshot};
use crate::mask::{self, Inspection, Inspections};
use crate::queue;
use crate::scan;
use secrethub_core::groups::{is_secret_name, valid_category, valid_variable_name, MAX_VARIABLES};
use secrethub_core::{
    GroupUpdate, NewGroup, NewItem, NewSecretInput, NewVariable, VariableUpdate, VaultService,
};
use secrethub_providers::ProviderDefinition;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

const INSPECT_PREFIX: &str = "inspect-";
const ORGANIZE_PREFIX: &str = "organize-";
const INSPECT_KIND: &str = "import_inspect";
const ORGANIZE_KIND: &str = "import_organize";
const MAX_PATHS: usize = 20;
const MAX_ITEMS: usize = 50;
const MAX_NAME_CHARS: usize = 100;
const MAX_DESCRIPTION_CHARS: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const LOGIN_FIELDS: [&str; 5] = ["website", "username", "password", "totp", "notes"];

// ------------------------------------------------------------------------ queue files

/// `inspect-<id>.json`: absolute paths only, like `import-<id>.json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInspect {
    #[serde(default)]
    pub version: u32,
    pub request_id: String,
    pub kind: String,
    pub paths: Vec<String>,
    pub created_at: String,
}

impl PendingInspect {
    fn validated(self, expected_id: &str) -> Result<Self, String> {
        if self.request_id != expected_id || !queue::valid_id(&self.request_id, INSPECT_PREFIX) {
            return Err("invalid inspect request: request id".to_owned());
        }
        if self.kind != INSPECT_KIND || self.version > 1 {
            return Err("invalid inspect request: kind or version".to_owned());
        }
        let bad_path = |path: &String| {
            path.is_empty()
                || path.len() > 400
                || path.chars().any(char::is_control)
                || !Path::new(path).is_absolute()
        };
        if self.paths.is_empty() || self.paths.len() > MAX_PATHS || self.paths.iter().any(bad_path)
        {
            return Err("invalid inspect request: paths".to_owned());
        }
        Ok(self)
    }
}

/// A value of a plan variable: a placeholder of the inspection, or text from the masked view.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlanValue {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub literal: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanVar {
    pub name: String,
    #[serde(default)]
    pub value: PlanValue,
    #[serde(default)]
    pub secret: Option<bool>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub variables: Vec<PlanVar>,
    #[serde(default)]
    pub notes: String,
}

/// `organize-<id>.json`: the AI's plan. Holds placeholder references and visible text only.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOrganize {
    #[serde(default)]
    pub version: u32,
    pub request_id: String,
    pub kind: String,
    pub inspection_id: String,
    pub items: Vec<PlanItem>,
    pub created_at: String,
}

impl PendingOrganize {
    fn validated(self, expected_id: &str) -> Result<Self, String> {
        if self.request_id != expected_id || !queue::valid_id(&self.request_id, ORGANIZE_PREFIX) {
            return Err("invalid organize request: request id".to_owned());
        }
        if self.kind != ORGANIZE_KIND || self.version > 1 {
            return Err("invalid organize request: kind or version".to_owned());
        }
        if !queue::valid_id(&self.inspection_id, INSPECT_PREFIX) {
            return Err("invalid organize request: inspection id".to_owned());
        }
        if self.items.is_empty() || self.items.len() > MAX_ITEMS {
            return Err("invalid organize request: items".to_owned());
        }
        Ok(self)
    }
}

fn read_queue<T>(
    directory: &Path,
    prefix: &str,
    parse: impl Fn(&str, &str) -> Option<(String, T)>,
) -> Vec<T> {
    let mut found: Vec<(String, T)> = queue::pending_files(directory, prefix)
        .into_iter()
        .filter_map(|(id, path)| parse(&id, &fs::read_to_string(path).ok()?))
        .collect();
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found.into_iter().map(|(_, request)| request).collect()
}

pub fn read_pending_inspects(directory: &Path) -> Vec<PendingInspect> {
    read_queue(directory, INSPECT_PREFIX, |id, text| {
        let request = serde_json::from_str::<PendingInspect>(text)
            .ok()?
            .validated(id)
            .ok()?;
        Some((request.created_at.clone(), request))
    })
}

pub fn read_pending_organizes(directory: &Path) -> Vec<PendingOrganize> {
    read_queue(directory, ORGANIZE_PREFIX, |id, text| {
        let request = serde_json::from_str::<PendingOrganize>(text)
            .ok()?
            .validated(id)
            .ok()?;
        Some((request.created_at.clone(), request))
    })
}

// ----------------------------------------------------------------------- inspection

#[derive(Debug, Serialize)]
struct FileView<'a> {
    name: &'a str,
    lines: &'a [String],
}

#[derive(Debug, Serialize)]
struct SkippedView<'a> {
    name: &'a str,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct ResultError {
    code: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectResult<'a> {
    request_id: &'a str,
    /// `done` | `failed`
    status: &'static str,
    inspection_id: &'a str,
    expires_in_seconds: u64,
    token_count: usize,
    truncated: bool,
    files: Vec<FileView<'a>>,
    skipped: Vec<SkippedView<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResultError>,
    finished_at: String,
}

/// Scans, masks and stores the inspection; writes the result file. Needs no vault.
pub fn run_inspect(
    request: &PendingInspect,
    inspections: &Inspections,
    results: &Path,
) -> Result<(), String> {
    let paths: Vec<PathBuf> = request.paths.iter().map(PathBuf::from).collect();
    let outcome = scan::read_files(&paths).map(|read| {
        let inspection = Inspection::build(&read.files, read.skipped);
        (inspection.files.is_empty(), inspection)
    });
    let finished_at = chrono::Utc::now().to_rfc3339();
    let failure = |code: &'static str| InspectResult {
        request_id: &request.request_id,
        status: "failed",
        inspection_id: &request.request_id,
        expires_in_seconds: 0,
        token_count: 0,
        truncated: false,
        files: Vec::new(),
        skipped: Vec::new(),
        error: Some(ResultError { code }),
        finished_at: finished_at.clone(),
    };
    match outcome {
        Ok((false, inspection)) => {
            let result = InspectResult {
                request_id: &request.request_id,
                status: "done",
                inspection_id: &request.request_id,
                expires_in_seconds: mask::TOKEN_TTL.as_secs(),
                token_count: inspection.token_count(),
                truncated: inspection.truncated,
                files: inspection
                    .files
                    .iter()
                    .map(|file| FileView {
                        name: &file.name,
                        lines: &file.lines,
                    })
                    .collect(),
                skipped: inspection
                    .skipped
                    .iter()
                    .map(|file| SkippedView {
                        name: &file.name,
                        reason: file.reason,
                    })
                    .collect(),
                error: None,
                finished_at: finished_at.clone(),
            };
            // The result is written before the inspection becomes usable, never the reverse.
            queue::write_result(results, &request.request_id, INSPECT_PREFIX, &result)
                .map_err(|error| error.to_string())?;
            inspections.insert(request.request_id.clone(), inspection);
            Ok(())
        }
        Ok((true, _)) => queue::write_result(
            results,
            &request.request_id,
            INSPECT_PREFIX,
            &failure("nothing_readable"),
        )
        .map_err(|error| error.to_string()),
        Err(_) => queue::write_result(
            results,
            &request.request_id,
            INSPECT_PREFIX,
            &failure("scan_failed"),
        )
        .map_err(|error| error.to_string()),
    }
}

// ------------------------------------------------------------------------ applying

/// A rejected item or variable: names and a reason code, never a value.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rejected {
    pub item: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    pub reason: &'static str,
}

/// What happened to one plan item.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub index: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// `created` | `added_vars` | `kept_both` | `skipped` | `rejected`
    pub result: &'static str,
    pub added_variables: usize,
    pub saved_separately: usize,
    pub skipped_variables: usize,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub created_groups: usize,
    pub added_variables: usize,
    pub saved_separately: usize,
    pub other_items: usize,
    pub skipped_variables: usize,
}

impl Counts {
    fn written(&self) -> usize {
        self.created_groups + self.added_variables + self.saved_separately + self.other_items
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanOutcome {
    /// `None` when nothing was written.
    pub batch_id: Option<String>,
    pub items: Vec<ItemResult>,
    pub rejected: Vec<Rejected>,
    pub counts: Counts,
}

struct Resolved {
    name: String,
    value: String,
    secret: bool,
    #[allow(dead_code)]
    description: String,
}

struct Applier<'a> {
    vault: &'a VaultService,
    inspection: &'a Inspection,
    registry: &'a [ProviderDefinition],
    batch: String,
    date: String,
    counts: Counts,
    rejected: Vec<Rejected>,
}

fn truncated_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME_CHARS).collect()
}

impl Applier<'_> {
    fn reject(&mut self, item: usize, variable: Option<&str>, reason: &'static str) {
        self.rejected.push(Rejected {
            item,
            variable: variable
                .filter(|name| valid_variable_name(name) && name.len() <= 64)
                .map(str::to_owned),
            reason,
        });
    }

    /// Value of a plan variable. A placeholder always yields a secret.
    fn resolve(&mut self, item: usize, var: &PlanVar) -> Option<Resolved> {
        let name = var.name.trim();
        let reject = |applier: &mut Self, reason| {
            applier.reject(item, Some(name), reason);
            None
        };
        let (value, from_token) = match (&var.value.token, &var.value.literal) {
            (Some(token), None) => {
                let Some(value) = mask::parse_token_ref(token)
                    .and_then(|number| self.inspection.token_value(number))
                else {
                    return reject(self, "unknown_token");
                };
                (value.to_owned(), true)
            }
            (None, Some(literal)) => {
                if !self.inspection.literal_allowed(literal) {
                    return reject(self, "literal_not_in_view");
                }
                (literal.clone(), false)
            }
            _ => return reject(self, "bad_value"),
        };
        if value.trim().is_empty() {
            return reject(self, "empty_value");
        }
        let description: String = var
            .description
            .trim()
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        if self.inspection.text_leaks_value(&description) {
            return reject(self, "description_has_value");
        }
        // Guard rail 1: a value from a placeholder is a secret, whatever the plan says.
        let secret = from_token || var.secret.unwrap_or_else(|| is_secret_name(name));
        Some(Resolved {
            name: name.to_owned(),
            value,
            secret,
            description,
        })
    }

    fn resolve_all(&mut self, item: usize, variables: &[PlanVar]) -> Vec<Resolved> {
        let mut resolved: Vec<Resolved> = Vec::new();
        for var in variables {
            let name = var.name.trim();
            if !valid_variable_name(name) {
                self.reject(item, None, "invalid_name");
                continue;
            }
            if resolved.iter().any(|existing| existing.name == name) {
                self.reject(item, Some(name), "duplicate_variable");
                continue;
            }
            if let Some(variable) = self.resolve(item, var) {
                resolved.push(variable);
            }
        }
        resolved
    }

    /// Notes may repeat visible words and cite placeholders (`«S3»`); real values never.
    fn notes(&mut self, item: usize, notes: &str) -> String {
        let notes = notes.trim();
        if notes.is_empty() {
            return String::new();
        }
        if self.inspection.text_leaks_value(notes) {
            self.reject(item, None, "notes_have_value");
            return String::new();
        }
        let mut out = String::new();
        let mut rest = notes;
        while let Some(start) = rest.find('«') {
            out.push_str(&rest[..start]);
            let Some(length) = rest[start..].find('»') else {
                out.push_str(&rest[start..]);
                rest = "";
                break;
            };
            let marker = &rest[start..start + length + '»'.len_utf8()];
            match mask::parse_token_ref(marker).and_then(|n| self.inspection.token_value(n)) {
                Some(value) => out.push_str(value),
                None => self.reject(item, None, "unknown_token"),
            }
            rest = &rest[start + length + '»'.len_utf8()..];
        }
        out.push_str(rest);
        out
    }

    fn record_group(&self, id: &str, names: &[&str], whole: bool) -> Result<(), String> {
        if whole {
            self.vault
                .record_batch_entry(&self.batch, id, None)
                .map_err(|e| e.to_string())?;
        }
        for name in names {
            self.vault
                .record_batch_entry(&self.batch, id, Some(name))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn new_group(
        &mut self,
        name: &str,
        category: &str,
        notes: &str,
        variables: &[Resolved],
    ) -> Result<String, String> {
        let id = self
            .vault
            .create_group(NewGroup {
                name: name.to_owned(),
                category: category.to_owned(),
                variables: variables
                    .iter()
                    .map(|variable| NewVariable {
                        name: variable.name.clone(),
                        value: variable.value.clone(),
                        secret: variable.secret,
                    })
                    .collect(),
                notes: notes.to_owned(),
                tags: vec!["imported".to_owned()],
                favorite: false,
                ai_values: true,
            })
            .map_err(|e| e.to_string())?;
        let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        self.record_group(&id, &names, true)?;
        Ok(id)
    }

    /// Appends variables to an existing group. Nothing already stored is changed.
    fn append(&mut self, group_id: &str, added: &[Resolved]) -> Result<(), String> {
        let metadata = self
            .vault
            .list_metadata()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|item| item.id == group_id)
            .ok_or_else(|| "group not found".to_owned())?;
        let mut variables: Vec<VariableUpdate> = self
            .vault
            .group_values(group_id, None)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(name, value, secret)| VariableUpdate {
                name,
                secret,
                value: (!secret).then_some(value),
                from: None,
            })
            .collect();
        variables.extend(added.iter().map(|variable| VariableUpdate {
            name: variable.name.clone(),
            secret: variable.secret,
            value: Some(variable.value.clone()),
            from: None,
        }));
        self.vault
            .update_group(
                group_id,
                GroupUpdate {
                    name: metadata.name.clone(),
                    category: metadata.category.clone(),
                    variables,
                    notes: None,
                    tags: metadata.tags.clone(),
                    favorite: metadata.favorite,
                    ai_values: metadata.ai_values,
                },
            )
            .map_err(|e| e.to_string())?;
        let names: Vec<&str> = added.iter().map(|v| v.name.as_str()).collect();
        self.record_group(group_id, &names, false)
    }

    /// Guard rail 2 for config groups: same-name group gets the missing variables; equal values
    /// are skipped; different values are saved as a new group and never overwrite.
    fn place_group(
        &mut self,
        name: &str,
        category: &str,
        notes: &str,
        variables: Vec<Resolved>,
        result: &mut ItemResult,
    ) -> Result<(), String> {
        let snap = snapshot(self.vault)?;
        let Some(existing) = snap.groups.iter().find(|g| same_name(&g.name, name)) else {
            self.new_group(name, category, notes, &variables)?;
            self.counts.created_groups += 1;
            result.result = "created";
            result.added_variables = variables.len();
            return Ok(());
        };
        let existing_id = existing.id.clone();
        let mut missing = Vec::new();
        let mut conflicting = Vec::new();
        for variable in variables {
            match existing
                .variables
                .iter()
                .find(|(n, _, _)| *n == variable.name)
            {
                None => missing.push(variable),
                Some((_, value, _)) if *value == variable.value => result.skipped_variables += 1,
                Some(_) => conflicting.push(variable),
            }
        }
        if !missing.is_empty() {
            self.append(&existing_id, &missing)?;
            self.counts.added_variables += missing.len();
            result.added_variables += missing.len();
        }
        if !conflicting.is_empty() {
            self.place_separately(name, category, conflicting, result)?;
        }
        self.counts.skipped_variables += result.skipped_variables;
        Ok(())
    }

    /// The conflicting variables go to `<name>（AI 导入 <date>）`, appended when that group exists
    /// already, in a further new group when they still differ from it.
    fn place_separately(
        &mut self,
        name: &str,
        category: &str,
        variables: Vec<Resolved>,
        result: &mut ItemResult,
    ) -> Result<(), String> {
        let dated = format!("{name}（AI 导入 {}）", self.date);
        let snap = snapshot(self.vault)?;
        let mut leftover = Vec::new();
        match snap.groups.iter().find(|g| same_name(&g.name, &dated)) {
            None => leftover = variables,
            Some(target) => {
                let target_id = target.id.clone();
                let mut missing = Vec::new();
                for variable in variables {
                    match target
                        .variables
                        .iter()
                        .find(|(n, _, _)| *n == variable.name)
                    {
                        None => missing.push(variable),
                        Some((_, value, _)) if *value == variable.value => {
                            result.skipped_variables += 1;
                        }
                        Some(_) => leftover.push(variable),
                    }
                }
                if !missing.is_empty() {
                    self.append(&target_id, &missing)?;
                    self.counts.saved_separately += missing.len();
                    result.saved_separately += missing.len();
                }
            }
        }
        if !leftover.is_empty() {
            let taken = snapshot(self.vault)?.names;
            let unique = unique_name(&dated, &taken);
            self.new_group(&unique, category, "", &leftover)?;
            self.counts.saved_separately += leftover.len();
            result.saved_separately += leftover.len();
        }
        Ok(())
    }

    fn apply_group(&mut self, index: usize, item: &PlanItem, result: &mut ItemResult) {
        let name = truncated_name(&item.name);
        if name.is_empty() {
            self.reject(index, None, "missing_name");
            return;
        }
        if item.variables.len() > MAX_VARIABLES {
            self.reject(index, None, "too_many_variables");
            return;
        }
        let variables = self.resolve_all(index, &item.variables);
        if variables.is_empty() {
            self.reject(index, None, "no_usable_variables");
            return;
        }
        let category = item
            .category
            .as_deref()
            .filter(|category| valid_category(category))
            .unwrap_or("other")
            .to_owned();
        let notes = self.notes(index, &item.notes);
        if let Err(_message) = self.place_group(&name, &category, &notes, variables, result) {
            self.reject(index, None, "write_failed");
        }
    }

    fn apply_api_key(&mut self, index: usize, item: &PlanItem, result: &mut ItemResult) {
        let Some(provider) = item
            .provider_id
            .as_deref()
            .and_then(|id| self.registry.iter().find(|provider| provider.id == id))
        else {
            self.reject(index, None, "unknown_provider");
            return;
        };
        let variables = self.resolve_all(index, &item.variables);
        let mut key: Option<Resolved> = None;
        let mut endpoint = None;
        let mut model = None;
        for variable in variables {
            if provider.model_env_key.as_deref() == Some(variable.name.as_str()) {
                model = Some(variable.value);
            } else if provider.base_url_env_key.as_deref() == Some(variable.name.as_str())
                || variable.name.ends_with("_BASE_URL")
            {
                endpoint = Some(variable.value);
            } else if key.is_none() {
                key = Some(variable);
            } else {
                self.reject(index, Some(&variable.name), "unsupported_variable");
            }
        }
        let Some(key) = key else {
            self.reject(index, None, "no_usable_variables");
            return;
        };
        let preset = crate::capture::build_preset(provider, "");
        let name = truncated_name(&item.name);
        let planned = PlannedSecret {
            name: if name.is_empty() {
                provider.name.clone()
            } else {
                name
            },
            provider_id: provider.id.clone(),
            env_key: key.name,
            value: key.value,
            provider_credential: true,
            endpoint_url: endpoint.unwrap_or(preset.endpoint_url),
            endpoint_env_key: preset.endpoint_env_key,
            model_id: model.unwrap_or(preset.model_id),
            model_env_key: preset.model_env_key,
            source_keys: Vec::new(),
        };
        let write = || -> Result<bool, String> {
            let snap = snapshot(self.vault)?;
            // Guard rail 2: the same value is never stored twice; a different one is kept next to it.
            if snap
                .keys
                .iter()
                .any(|existing| existing.value == planned.value)
            {
                return Ok(false);
            }
            let name = unique_name(&planned.name, &snap.names);
            let attributes = crate::commands::import_attributes(&planned);
            let id = self
                .vault
                .create_secret_with_attributes(
                    NewSecretInput {
                        name,
                        provider_id: planned.provider_id.clone(),
                        env_key: planned.env_key.clone(),
                        description: "Imported by AI".to_owned(),
                        tags: vec!["imported".to_owned()],
                        value: planned.value.clone(),
                    },
                    attributes,
                )
                .map_err(|e| e.to_string())?;
            self.vault
                .record_batch_entry(&self.batch, &id, None)
                .map_err(|e| e.to_string())?;
            Ok(true)
        };
        match write() {
            Ok(true) => {
                self.counts.other_items += 1;
                result.result = "created";
            }
            Ok(false) => {}
            Err(_) => self.reject(index, None, "write_failed"),
        }
    }

    fn apply_login(&mut self, index: usize, item: &PlanItem, result: &mut ItemResult) {
        let mut fields = BTreeMap::new();
        for variable in self.resolve_all(index, &item.variables) {
            if LOGIN_FIELDS.contains(&variable.name.as_str()) {
                fields.insert(variable.name, variable.value);
            } else {
                self.reject(index, Some(&variable.name), "unsupported_field");
            }
        }
        let name = truncated_name(&item.name);
        if name.is_empty() || !fields.contains_key("username") {
            self.reject(index, None, "missing_name_or_username");
            return;
        }
        let write = || -> Result<bool, String> {
            let snap = snapshot(self.vault)?;
            let same = snap.typed.iter().any(|existing| {
                existing.metadata.value_type == "login"
                    && fields
                        .iter()
                        .all(|(key, value)| existing.fields.get(key) == Some(value))
            });
            if same {
                return Ok(false);
            }
            let id = self
                .vault
                .create_item(NewItem {
                    item_type: "login".to_owned(),
                    name: unique_name(&name, &snap.names),
                    fields: fields.clone(),
                    tags: vec!["imported".to_owned()],
                    favorite: false,
                    dev_use: false,
                    env_key: String::new(),
                })
                .map_err(|e| e.to_string())?;
            self.vault
                .record_batch_entry(&self.batch, &id, None)
                .map_err(|e| e.to_string())?;
            Ok(true)
        };
        match write() {
            Ok(true) => {
                self.counts.other_items += 1;
                result.result = "created";
            }
            Ok(false) => {}
            Err(_) => self.reject(index, None, "write_failed"),
        }
    }

    fn apply_note(&mut self, index: usize, item: &PlanItem, result: &mut ItemResult) {
        let name = truncated_name(&item.name);
        let body = self.notes(index, &item.notes);
        if name.is_empty() || body.is_empty() {
            self.reject(index, None, "missing_name_or_body");
            return;
        }
        let write = || -> Result<bool, String> {
            let snap: Snapshot = snapshot(self.vault)?;
            if snap.typed.iter().any(|existing| {
                existing.metadata.value_type == "note" && existing.fields.get("body") == Some(&body)
            }) {
                return Ok(false);
            }
            let fields = BTreeMap::from([("body".to_owned(), body.clone())]);
            let id = self
                .vault
                .create_item(NewItem {
                    item_type: "note".to_owned(),
                    name: unique_name(&name, &snap.names),
                    fields,
                    tags: vec!["imported".to_owned()],
                    favorite: false,
                    dev_use: false,
                    env_key: String::new(),
                })
                .map_err(|e| e.to_string())?;
            self.vault
                .record_batch_entry(&self.batch, &id, None)
                .map_err(|e| e.to_string())?;
            Ok(true)
        };
        match write() {
            Ok(true) => {
                self.counts.other_items += 1;
                result.result = "created";
            }
            Ok(false) => {}
            Err(_) => self.reject(index, None, "write_failed"),
        }
    }
}

/// Writes an AI plan into the vault under the guard rails. The vault must be unlocked.
pub fn apply_plan(
    vault: &VaultService,
    inspection: &Inspection,
    items: &[PlanItem],
    registry: &[ProviderDefinition],
    date: &str,
) -> Result<PlanOutcome, String> {
    let batch = vault.begin_import_batch().map_err(|e| e.to_string())?;
    let mut applier = Applier {
        vault,
        inspection,
        registry,
        batch: batch.clone(),
        date: date.to_owned(),
        counts: Counts::default(),
        rejected: Vec::new(),
    };
    let mut results = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut result = ItemResult {
            index,
            name: truncated_name(&item.name),
            kind: item.kind.chars().take(32).collect(),
            result: "skipped",
            added_variables: 0,
            saved_separately: 0,
            skipped_variables: 0,
        };
        let rejected_before = applier.rejected.len();
        match item.kind.as_str() {
            "env_group" => applier.apply_group(index, item, &mut result),
            "api_key" => applier.apply_api_key(index, item, &mut result),
            "login" => applier.apply_login(index, item, &mut result),
            "note" => applier.apply_note(index, item, &mut result),
            _ => applier.reject(index, None, "unsupported_type"),
        }
        if result.result != "created" {
            result.result = if result.added_variables > 0 {
                "added_vars"
            } else if result.saved_separately > 0 {
                "kept_both"
            } else if applier.rejected.len() > rejected_before
                && applier.rejected[rejected_before..]
                    .iter()
                    .any(|r| r.variable.is_none())
            {
                "rejected"
            } else {
                "skipped"
            };
        }
        results.push(result);
    }
    let counts = applier.counts.clone();
    let rejected = std::mem::take(&mut applier.rejected);
    let batch_id = if counts.written() == 0 {
        // Nothing was written: no notice, no batch to undo.
        let _ = vault.dismiss_import_batch(&batch);
        None
    } else {
        vault
            .finish_import_batch(
                &batch,
                [
                    counts.created_groups as i64,
                    counts.added_variables as i64,
                    counts.saved_separately as i64,
                    counts.other_items as i64,
                ],
            )
            .map_err(|e| e.to_string())?;
        let _ = vault.record_action(
            "ai_import_applied",
            None,
            &serde_json::json!({
                "source": "mcp",
                "batch": batch,
                "createdGroups": counts.created_groups,
                "addedVariables": counts.added_variables,
                "savedSeparately": counts.saved_separately,
                "otherItems": counts.other_items
            })
            .to_string(),
        );
        Some(batch)
    };
    Ok(PlanOutcome {
        batch_id,
        items: results,
        rejected,
        counts,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganizeResult<'a> {
    request_id: &'a str,
    /// `done` | `failed`
    status: &'static str,
    inspection_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    batch_id: Option<&'a str>,
    items: &'a [ItemResult],
    rejected: &'a [Rejected],
    counts: &'a Counts,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResultError>,
    finished_at: String,
}

/// Runs one organize request against the vault and writes its result file. Returns `false`
/// when the request cannot run yet (it stays pending).
pub fn run_organize(
    request: &PendingOrganize,
    vault: &VaultService,
    inspections: &Inspections,
    registry: &[ProviderDefinition],
    results: &Path,
    now: Instant,
    date: &str,
) -> Result<(), String> {
    let empty_counts = Counts::default();
    let failed = |code: &'static str| OrganizeResult {
        request_id: &request.request_id,
        status: "failed",
        inspection_id: &request.inspection_id,
        batch_id: None,
        items: &[],
        rejected: &[],
        counts: &empty_counts,
        error: Some(ResultError { code }),
        finished_at: chrono::Utc::now().to_rfc3339(),
    };
    let write = |value: &OrganizeResult| {
        queue::write_result(results, &request.request_id, ORGANIZE_PREFIX, value)
            .map_err(|error| error.to_string())
    };
    let Some(inspection) = inspections.get(&request.inspection_id, now) else {
        return write(&failed("inspection_expired"));
    };
    match apply_plan(vault, &inspection, &request.items, registry, date) {
        Ok(outcome) => write(&OrganizeResult {
            request_id: &request.request_id,
            status: "done",
            inspection_id: &request.inspection_id,
            batch_id: outcome.batch_id.as_deref(),
            items: &outcome.items,
            rejected: &outcome.rejected,
            counts: &outcome.counts,
            error: None,
            finished_at: chrono::Utc::now().to_rfc3339(),
        }),
        Err(_) => write(&failed("import_failed")),
    }
}

// ------------------------------------------------------------------ background loop

fn directories(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok((data.join("mcp-pending"), data.join("mcp-results")))
}

/// Inspect requests run at once; organize requests when the vault is usable.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            tick(&app);
        }
    });
}

fn tick(app: &AppHandle) {
    let Ok((pending, results)) = directories(app) else {
        return;
    };
    let state = app.state::<crate::AppState>();
    for request in read_pending_inspects(&pending) {
        if run_inspect(&request, &state.inspections, &results).is_ok() {
            let _ = fs::remove_file(pending.join(format!("{}.json", request.request_id)));
        }
    }
    let organizes = read_pending_organizes(&pending);
    if organizes.is_empty() {
        return;
    }
    let registry = crate::catalog::effective_registry();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    for request in organizes {
        if !crate::sync::ready(&state) {
            return; // stays pending until the vault is unlocked
        }
        let done = {
            let Ok(guard) = state.vault.lock() else {
                return;
            };
            let Some(vault) = guard.as_ref() else {
                return;
            };
            run_organize(
                &request,
                vault,
                &state.inspections,
                &registry,
                &results,
                Instant::now(),
                &date,
            )
        };
        if done.is_ok() {
            let _ = fs::remove_file(pending.join(format!("{}.json", request.request_id)));
        }
    }
}

// -------------------------------------------------------------------------- commands

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOrganizeView {
    pub request_id: String,
    pub item_count: usize,
    pub created_at: String,
}

/// Plans waiting for the vault to be unlocked (banner "writes automatically after unlocking").
#[tauri::command]
pub fn mcp_pending_organize_requests(app: AppHandle) -> Result<Vec<PendingOrganizeView>, String> {
    let (pending, _) = directories(&app)?;
    Ok(read_pending_organizes(&pending)
        .into_iter()
        .map(|request| PendingOrganizeView {
            request_id: request.request_id,
            item_count: request.items.len(),
            created_at: request.created_at,
        })
        .collect())
}

fn with_vault<T>(
    state: &tauri::State<'_, crate::AppState>,
    action: impl FnOnce(&VaultService) -> Result<T, String>,
) -> Result<T, String> {
    crate::commands::touch_activity(state)?;
    let guard = crate::commands::vault_from_state(state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    action(vault)
}

/// Notices of automatic AI imports that the user has not dismissed or undone.
#[tauri::command]
pub fn ai_import_notices(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<secrethub_storage::ImportBatch>, String> {
    with_vault(&state, |vault| {
        vault.open_import_batches().map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn ai_import_dismiss(
    batch_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    with_vault(&state, |vault| {
        vault
            .dismiss_import_batch(&batch_id)
            .map_err(|e| e.to_string())
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoView {
    pub removed_items: usize,
    pub removed_variables: usize,
}

/// Removes exactly what the batch added.
#[tauri::command]
pub fn ai_import_undo(
    batch_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<UndoView, String> {
    with_vault(&state, |vault| {
        vault
            .undo_import_batch(&batch_id)
            .map(|summary| UndoView {
                removed_items: summary.removed_items,
                removed_variables: summary.removed_variables,
            })
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::FileInput;
    use secrethub_storage::Database;

    // Obvious fakes; none of them may appear in a result file, an audit event or an error.
    const HOST: &str = "8.0.0.1";
    const PASSWORD: &str = "fakeval-ali-ssh-password";
    const OTHER_PASSWORD: &str = "fakeval-other-ssh-password";
    const API_KEY: &str = "sk-fakeval-organize-key";
    const OTHER_KEY: &str = "sk-fakeval-organize-key-2";
    const PRIVATE_KEY: &str =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEVALORGANIZE\n-----END OPENSSH PRIVATE KEY-----";
    const FAKES: [&str; 6] = [
        PASSWORD,
        OTHER_PASSWORD,
        API_KEY,
        OTHER_KEY,
        "FAKEVALORGANIZE",
        "fakeval-login-password",
    ];
    const DATE: &str = "2026-11-10";

    fn vault(directory: &Path) -> VaultService {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        vault
    }

    fn file(name: &str, text: &str) -> FileInput {
        FileInput {
            name: name.to_owned(),
            text: text.to_owned(),
        }
    }

    fn provider() -> ProviderDefinition {
        crate::catalog::effective_registry()
            .into_iter()
            .find(|p| p.model_env_key.is_some() && !p.env_keys.is_empty())
            .expect("a model provider")
    }

    fn inspection(password: &str, key: &str) -> Inspection {
        let provider = provider();
        Inspection::build(
            &[
                file(
                    "阿里宝塔/.env",
                    &format!(
                        "ALI_SSH_HOST={HOST}\nALI_SSH_PORT=22\nALI_SSH_USER=root\nALI_SSH_PASSWORD={password}\nALI_SSH_PRIVATE_KEY=\"{PRIVATE_KEY}\"\n{}={key}\n",
                        provider.env_keys[0]
                    ),
                ),
                file(
                    "阿里宝塔/账号.txt",
                    "登录账号 admin 密码 fakeval-login-password\n",
                ),
            ],
            Vec::new(),
        )
    }

    /// `S<n>` of the placeholder that stands in `NAME=` of the view.
    fn token(inspection: &Inspection, name: &str) -> String {
        let prefix = format!("{name}=«S");
        for line in inspection.files.iter().flat_map(|f| &f.lines) {
            if let Some(rest) = line.strip_prefix(&prefix) {
                let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
                return format!("S{number}");
            }
        }
        panic!("no placeholder for {name}");
    }

    fn var(name: &str, token: Option<&str>, literal: Option<&str>) -> PlanVar {
        PlanVar {
            name: name.to_owned(),
            value: PlanValue {
                token: token.map(str::to_owned),
                literal: literal.map(str::to_owned),
            },
            secret: None,
            description: String::new(),
        }
    }

    fn group_item(name: &str, variables: Vec<PlanVar>) -> PlanItem {
        PlanItem {
            kind: "env_group".into(),
            name: name.into(),
            category: Some("server".into()),
            provider_id: None,
            variables,
            notes: String::new(),
        }
    }

    fn ali_group(inspection: &Inspection) -> PlanItem {
        group_item(
            "阿里宝塔服务器",
            vec![
                var("ALI_BT_SSH_HOST", None, Some(HOST)),
                var("ALI_BT_SSH_PORT", None, Some("22")),
                var(
                    "ALI_BT_SSH_PASSWORD",
                    Some(&token(inspection, "ALI_SSH_PASSWORD")),
                    None,
                ),
                var(
                    "ALI_BT_SSH_PRIVATE_KEY",
                    Some(&token(inspection, "ALI_SSH_PRIVATE_KEY")),
                    None,
                ),
            ],
        )
    }

    fn apply(vault: &VaultService, inspection: &Inspection, items: &[PlanItem]) -> PlanOutcome {
        apply_plan(
            vault,
            inspection,
            items,
            &crate::catalog::effective_registry(),
            DATE,
        )
        .unwrap()
    }

    fn group_id(vault: &VaultService, name: &str) -> Option<String> {
        vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|item| item.value_type == "env_group" && item.name == name)
            .map(|item| item.id)
    }

    fn values(vault: &VaultService, name: &str) -> Vec<(String, String, bool)> {
        vault
            .group_values(&group_id(vault, name).unwrap(), None)
            .unwrap()
    }

    fn assert_clean(label: &str, text: &str) {
        for fake in FAKES {
            assert!(!text.contains(fake), "{label} leaks {fake}");
        }
    }

    #[test]
    fn creates_a_group_with_tokens_as_secrets_even_when_the_plan_says_plain() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let view = inspection(PASSWORD, API_KEY);
        let mut item = ali_group(&view);
        // Guard rail 1: secret:false is ignored for a value that came from a placeholder.
        item.variables[2].secret = Some(false);
        item.variables[0].secret = Some(true); // a literal follows the plan's flag
        let outcome = apply(&vault, &view, &[item]);
        assert_eq!(outcome.items[0].result, "created");
        assert_eq!(outcome.counts.created_groups, 1);
        let stored = values(&vault, "阿里宝塔服务器");
        let flag = |name: &str| stored.iter().find(|v| v.0 == name).unwrap().2;
        assert!(flag("ALI_BT_SSH_PASSWORD") && flag("ALI_BT_SSH_PRIVATE_KEY"));
        assert!(flag("ALI_BT_SSH_HOST"), "literal honours the plan");
        assert!(
            !flag("ALI_BT_SSH_PORT"),
            "literal without a flag follows the name rule"
        );
        let value = |name: &str| stored.iter().find(|v| v.0 == name).unwrap().1.clone();
        assert_eq!(value("ALI_BT_SSH_PASSWORD"), PASSWORD);
        assert!(value("ALI_BT_SSH_PRIVATE_KEY").contains("FAKEVALORGANIZE"));
        let json = serde_json::to_string(&outcome).unwrap();
        assert_clean("outcome", &json);
        let audit = serde_json::to_string(&vault.list_audit().unwrap()).unwrap();
        assert_clean("audit", &audit);
        assert!(audit.contains("ai_import_applied"));
    }

    #[test]
    fn same_name_group_only_grows_and_conflicts_are_saved_as_a_new_group() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let first = inspection(PASSWORD, API_KEY);
        apply(&vault, &first, &[ali_group(&first)]);
        let before = values(&vault, "阿里宝塔服务器");

        // Second import: same group name, one new variable, equal ones, one with another value.
        let second = inspection(OTHER_PASSWORD, API_KEY);
        let mut item = ali_group(&second);
        item.variables
            .push(var("ALI_BT_SSH_USER", None, Some("root")));
        let outcome = apply(&vault, &second, &[item]);
        let result = &outcome.items[0];
        assert_eq!(result.result, "added_vars", "{result:?}");
        assert_eq!(result.added_variables, 1, "{result:?}");
        assert_eq!(result.saved_separately, 1, "{result:?}"); // the other password
        assert_eq!(result.skipped_variables, 3, "{result:?}"); // host, port, same key

        // Nothing that existed was changed.
        let after = values(&vault, "阿里宝塔服务器");
        for old in &before {
            assert_eq!(after.iter().find(|v| v.0 == old.0), Some(old), "{old:?}");
        }
        assert_eq!(after.len(), before.len() + 1);
        let dated = values(&vault, &format!("阿里宝塔服务器（AI 导入 {DATE}）"));
        assert_eq!(
            dated,
            [("ALI_BT_SSH_PASSWORD".into(), OTHER_PASSWORD.into(), true)]
        );

        // The same plan again changes nothing at all.
        let again = apply(&vault, &second, &[ali_group(&second)]);
        assert_eq!(again.batch_id, None);
        assert_eq!(again.items[0].result, "skipped");
        assert_eq!(values(&vault, "阿里宝塔服务器").len(), after.len());
        let groups = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .filter(|i| i.value_type == "env_group")
            .count();
        assert_eq!(groups, 2);
    }

    fn api_key_item(view: &Inspection) -> PlanItem {
        let provider = provider();
        PlanItem {
            kind: "api_key".into(),
            name: String::new(),
            category: None,
            provider_id: Some(provider.id.clone()),
            variables: vec![var(
                &provider.env_keys[0],
                Some(&token(view, &provider.env_keys[0])),
                None,
            )],
            notes: String::new(),
        }
    }

    #[test]
    fn api_keys_are_deduplicated_by_value_and_kept_twice_when_they_differ() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let first = inspection(PASSWORD, API_KEY);
        let outcome = apply(&vault, &first, &[api_key_item(&first)]);
        assert_eq!(outcome.items[0].result, "created");

        let same = apply(&vault, &first, &[api_key_item(&first)]);
        assert_eq!(same.items[0].result, "skipped");
        assert_eq!(same.batch_id, None);

        let other = inspection(PASSWORD, OTHER_KEY);
        let both = apply(&vault, &other, &[api_key_item(&other)]);
        assert_eq!(both.items[0].result, "created");
        let keys: Vec<_> = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .filter(|item| item.value_type == "api_key")
            .collect();
        assert_eq!(keys.len(), 2, "both keys are kept");
        let mut stored: Vec<String> = keys
            .iter()
            .map(|k| vault.read_secret(&k.id).unwrap())
            .collect();
        stored.sort();
        assert_eq!(stored, [API_KEY, OTHER_KEY]);
        assert_ne!(keys[0].name, keys[1].name);
        assert_clean("outcome", &serde_json::to_string(&both).unwrap());
    }

    #[test]
    fn logins_and_notes_are_created_once() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let view = inspection(PASSWORD, API_KEY);
        let login = PlanItem {
            kind: "login".into(),
            name: "阿里账号".into(),
            category: None,
            provider_id: None,
            variables: vec![
                var("username", None, Some("admin")),
                var("password", Some("S99"), None),
            ],
            notes: String::new(),
        };
        // S99 does not exist: the variable is rejected, the login (username only) is still made.
        let outcome = apply(&vault, &view, std::slice::from_ref(&login));
        assert_eq!(outcome.items[0].result, "created");
        assert!(outcome.rejected.iter().any(|r| r.reason == "unknown_token"));
        assert_eq!(
            apply(&vault, &view, std::slice::from_ref(&login)).items[0].result,
            "skipped"
        );
        let note = PlanItem {
            kind: "note".into(),
            name: "说明".into(),
            category: None,
            provider_id: None,
            variables: Vec::new(),
            notes: format!("主机 {HOST}，端口 22"),
        };
        assert_eq!(
            apply(&vault, &view, std::slice::from_ref(&note)).items[0].result,
            "created"
        );
        assert_eq!(
            apply(&vault, &view, std::slice::from_ref(&note)).items[0].result,
            "skipped"
        );
    }

    #[test]
    fn bad_literals_tokens_names_and_descriptions_are_rejected_with_reasons_only() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let view = inspection(PASSWORD, API_KEY);
        let mut leaking = var("ALI_NOTE", None, Some(HOST));
        leaking.description = format!("密码是 {PASSWORD}");
        let item = group_item(
            "校验",
            vec![
                var("OK_HOST", None, Some(HOST)),
                var("INVENTED_HOST", None, Some("9.9.9.9")),
                var("GUESSED_PASSWORD", None, Some(PASSWORD)),
                var("BAD-NAME", None, Some(HOST)),
                var("NO_TOKEN", Some("S77"), None),
                var("BOTH", Some("S1"), Some(HOST)),
                leaking,
                var("OK_HOST", None, Some(HOST)),
            ],
        );
        let outcome = apply(&vault, &view, &[item]);
        let reasons: Vec<_> = outcome.rejected.iter().map(|r| r.reason).collect();
        for expected in [
            "literal_not_in_view",
            "invalid_name",
            "unknown_token",
            "bad_value",
            "description_has_value",
            "duplicate_variable",
        ] {
            assert!(reasons.contains(&expected), "{expected} in {reasons:?}");
        }
        assert_eq!(values(&vault, "校验").len(), 1);
        assert_clean("outcome", &serde_json::to_string(&outcome).unwrap());
    }

    #[test]
    fn undo_removes_only_what_the_batch_added() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        // A group the user already had.
        let existing = vault
            .create_group(NewGroup {
                name: "阿里宝塔服务器".into(),
                category: "server".into(),
                variables: vec![NewVariable {
                    name: "ALI_BT_SSH_HOST".into(),
                    value: HOST.into(),
                    secret: false,
                }],
                notes: "keep me".into(),
                tags: Vec::new(),
                favorite: false,
                ai_values: true,
            })
            .unwrap();
        let view = inspection(PASSWORD, API_KEY);
        let mut other = ali_group(&view);
        other.name = "另一台".into();
        let items = vec![ali_group(&view), other, api_key_item(&view)];
        let outcome = apply(&vault, &view, &items);
        let batch = outcome.batch_id.clone().unwrap();
        assert_eq!(outcome.counts.created_groups, 1);
        assert_eq!(outcome.counts.added_variables, 3);
        assert_eq!(outcome.counts.other_items, 1);
        assert_eq!(values(&vault, "阿里宝塔服务器").len(), 4);
        let open = vault.open_import_batches().unwrap();
        assert_eq!(open.len(), 1);

        let summary = vault.undo_import_batch(&batch).unwrap();
        assert_eq!(summary.removed_items, 2); // the new group and the new key
        assert_eq!(summary.removed_variables, 3); // the ones appended to the user's group
        assert_eq!(group_id(&vault, "另一台"), None);
        assert!(vault
            .list_metadata()
            .unwrap()
            .iter()
            .all(|item| item.value_type != "api_key"));
        // The user's group is back to exactly what it was, notes included.
        assert_eq!(
            values(&vault, "阿里宝塔服务器"),
            [("ALI_BT_SSH_HOST".to_owned(), HOST.to_owned(), false)]
        );
        assert_eq!(vault.group_notes(&existing).unwrap(), "keep me");
        assert!(vault.open_import_batches().unwrap().is_empty());
        // A batch can be undone once.
        assert!(vault.undo_import_batch(&batch).is_err());
        assert_clean(
            "audit",
            &serde_json::to_string(&vault.list_audit().unwrap()).unwrap(),
        );
    }

    #[test]
    fn undo_keeps_a_group_that_a_later_batch_built_on() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let view = inspection(PASSWORD, API_KEY);
        let first = apply(&vault, &view, &[ali_group(&view)]);
        let mut more = group_item(
            "阿里宝塔服务器",
            vec![var("ALI_BT_SSH_USER", None, Some("root"))],
        );
        more.category = None;
        let second = apply(&vault, &view, &[more]);
        assert_eq!(second.counts.added_variables, 1);

        vault.undo_import_batch(&first.batch_id.unwrap()).unwrap();
        // The group survives with only the later batch's variable.
        assert_eq!(
            values(&vault, "阿里宝塔服务器"),
            [("ALI_BT_SSH_USER".to_owned(), "root".to_owned(), false)]
        );
        vault.undo_import_batch(&second.batch_id.unwrap()).unwrap();
    }

    #[test]
    fn expired_inspections_fail_the_request_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let results = dir.path().join("results");
        let inspections = Inspections::default();
        let request = PendingOrganize {
            version: 1,
            request_id: "organize-1".into(),
            kind: ORGANIZE_KIND.into(),
            inspection_id: "inspect-1".into(),
            items: vec![group_item("x", vec![var("A", None, Some("1"))])],
            created_at: "now".into(),
        };
        run_organize(
            &request,
            &vault,
            &inspections,
            &[],
            &results,
            Instant::now(),
            DATE,
        )
        .unwrap();
        let text = fs::read_to_string(results.join("organize-1.json")).unwrap();
        assert!(text.contains("inspection_expired"), "{text}");
        assert!(vault.list_metadata().unwrap().is_empty());

        // A stored inspection expires after the time to live.
        let start = Instant::now();
        inspections.insert(
            "inspect-1".into(),
            Inspection::build_at(&[file("a/.env", "A=1\n")], Vec::new(), start),
        );
        let late = start + mask::TOKEN_TTL + Duration::from_secs(1);
        run_organize(&request, &vault, &inspections, &[], &results, late, DATE).unwrap();
        assert!(fs::read_to_string(results.join("organize-1.json"))
            .unwrap()
            .contains("inspection_expired"));
    }

    #[test]
    fn inspect_requests_write_a_masked_result_and_queue_kinds_stay_apart() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("资料");
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join(".env"),
            format!("SSH_HOST={HOST}\nSSH_PASSWORD={PASSWORD}\nTOKEN_X={API_KEY}\n"),
        )
        .unwrap();
        fs::write(folder.join("id_ed25519"), format!("{PRIVATE_KEY}\n")).unwrap();
        let pending = dir.path().join("pending");
        let results = dir.path().join("results");
        fs::create_dir_all(&pending).unwrap();
        let body = |id: &str, kind: &str, extra: &str| {
            format!(
                r#"{{"version":1,"requestId":"{id}","kind":"{kind}",{extra}"createdAt":"2026-11-10T00:00:00Z"}}"#
            )
        };
        let path = serde_json::to_string(&folder.to_string_lossy()).unwrap();
        fs::write(
            pending.join("inspect-1.json"),
            body("inspect-1", INSPECT_KIND, &format!(r#""paths":[{path}],"#)),
        )
        .unwrap();
        fs::write(
            pending.join("organize-1.json"),
            body(
                "organize-1",
                ORGANIZE_KIND,
                r#""inspectionId":"inspect-1","items":[{"type":"note","name":"n","notes":"x"}],"#,
            ),
        )
        .unwrap();
        fs::write(
            pending.join("inspect-bad.json"),
            body("inspect-other", INSPECT_KIND, r#""paths":["relative"],"#),
        )
        .unwrap();
        fs::write(
            pending.join("import-1.json"),
            body("import-1", "import", &format!(r#""paths":[{path}],"#)),
        )
        .unwrap();
        // Each reader sees only its own kind.
        assert_eq!(read_pending_inspects(&pending).len(), 1);
        assert_eq!(read_pending_organizes(&pending).len(), 1);
        assert_eq!(crate::import_flow::read_pending(&pending).len(), 1);

        let inspections = Inspections::default();
        let request = read_pending_inspects(&pending).remove(0);
        run_inspect(&request, &inspections, &results).unwrap();
        let text = fs::read_to_string(results.join("inspect-1.json")).unwrap();
        assert_clean("inspection result", &text);
        assert!(text.contains(HOST) && text.contains("«S"), "{text}");
        let stored = inspections.get("inspect-1", Instant::now()).unwrap();
        assert!(stored.token_count() >= 3);
    }
}
