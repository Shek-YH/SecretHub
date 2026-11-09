//! Path-based import assistant (design section 3): scan the files the user (or the AI, by path
//! only) pointed at, compare the candidates with the vault, let the user review names and
//! choices, then write. Candidate values live in a Desktop-side session and never reach the web
//! view, the MCP result files, logs or error texts.

use crate::import::{assign_names, plan_import, PlannedSecret};
use crate::scan::{self, ScanVar, ScannedGroup, SkippedFile};
use secrethub_core::{
    GroupUpdate, NewGroup, NewSecretInput, NewVariable, VariableUpdate, VaultService,
};
use secrethub_exporter::EnvEntry;
use secrethub_providers::ProviderDefinition;
use secrethub_storage::SecretMetadata;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
};

// ------------------------------------------------------------------------ candidates

/// A candidate config group and the provider keys that can be split off it.
pub struct GroupCand {
    pub name: String,
    pub category: String,
    pub variables: Vec<ScanVar>,
    pub notes: String,
}

pub struct ApiKeyCand {
    pub planned: PlannedSecret,
    /// Index of the group item this key was found in.
    pub group_item: usize,
    /// Variables of the group that make up this key (key, base URL, model).
    pub group_vars: Vec<String>,
}

pub enum ItemData {
    Group(GroupCand),
    ApiKey(ApiKeyCand),
}

pub struct Item {
    pub id: String,
    pub source: String,
    pub data: ItemData,
}

/// What the web view may know about a scan: names, flags and choices, no values.
pub struct Session {
    pub request_id: Option<String>,
    pub items: Vec<Item>,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Default)]
pub struct Sessions {
    inner: Mutex<HashMap<String, Session>>,
}

const MAX_SESSIONS: usize = 4;

impl Sessions {
    pub fn insert(&self, id: String, session: Session) -> Result<(), String> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| "import state is unavailable".to_owned())?;
        // Old reviews that were never finished are dropped, oldest id first.
        while map.len() >= MAX_SESSIONS {
            let Some(oldest) = map.keys().min().cloned() else {
                break;
            };
            map.remove(&oldest);
        }
        map.insert(id, session);
        Ok(())
    }

    pub fn take(&self, id: &str) -> Result<Option<Session>, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "import state is unavailable".to_owned())?
            .remove(id))
    }

    pub fn put_back(&self, id: String, session: Session) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(id, session);
        }
    }
}

/// Category guess for a new group from its variable names.
pub fn guess_category(variables: &[ScanVar]) -> &'static str {
    let has = |prefixes: &[&str]| {
        variables.iter().any(|var| {
            let upper = var.name.to_ascii_uppercase();
            prefixes.iter().any(|prefix| upper.starts_with(prefix))
        })
    };
    if has(&["SSH_", "BT_PANEL", "SERVER_"]) {
        "server"
    } else if has(&["DB_", "MYSQL_", "POSTGRES", "PG", "REDIS_", "MONGO"]) {
        "database"
    } else if has(&["OSS_", "S3_", "COS_", "BUCKET"]) {
        "storage"
    } else {
        "other"
    }
}

fn last_value(var: &ScanVar) -> &str {
    &var.values[var.last_index()]
}

/// Turns scanned groups into review items: one group item each, followed by the provider keys
/// found in it (default: split off as API keys).
pub fn build_items(groups: Vec<ScannedGroup>, registry: &[ProviderDefinition]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for group in groups {
        let group_index = items.len();
        let entries: Vec<EnvEntry> = group
            .variables
            .iter()
            .map(|var| EnvEntry::new(&var.name, last_value(var)))
            .collect();
        let keys: Vec<PlannedSecret> = plan_import(&entries, registry)
            .into_iter()
            .filter(|secret| secret.provider_credential)
            .collect();
        let category = guess_category(&group.variables).to_owned();
        items.push(Item {
            id: format!("i{group_index}"),
            source: group.source.clone(),
            data: ItemData::Group(GroupCand {
                name: group.name.clone(),
                category,
                variables: group.variables,
                notes: group.notes,
            }),
        });
        for planned in keys {
            let id = format!("i{}", items.len());
            items.push(Item {
                id,
                source: group.source.clone(),
                data: ItemData::ApiKey(ApiKeyCand {
                    group_vars: planned.source_keys.clone(),
                    planned,
                    group_item: group_index,
                }),
            });
        }
    }
    items
}

// ---------------------------------------------------------------------- decisions

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDecision {
    pub id: String,
    /// Unchecked items are not imported.
    #[serde(default = "yes")]
    pub include: bool,
    /// Dedupe choice (`merge`, `copy`, `skip`, `overwrite`, `keep_both`); unknown means default.
    #[serde(default)]
    pub action: Option<String>,
    /// API keys: `false` keeps the variables in the group instead of splitting them off.
    #[serde(default)]
    pub split: Option<bool>,
    /// Which duplicate value to keep, by variable name (index into the distinct values).
    #[serde(default)]
    pub picks: HashMap<String, usize>,
    /// Overrides of the secret flag, by variable name.
    #[serde(default)]
    pub secrets: HashMap<String, bool>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDecisions {
    #[serde(default)]
    pub items: Vec<ItemDecision>,
}

impl ImportDecisions {
    fn for_item(&self, id: &str) -> ItemDecision {
        self.items
            .iter()
            .find(|decision| decision.id == id)
            .cloned()
            .unwrap_or_else(|| ItemDecision {
                id: id.to_owned(),
                include: true,
                ..Default::default()
            })
    }
}

// ------------------------------------------------------------------ vault snapshot

struct ExistingGroup {
    id: String,
    name: String,
    variables: Vec<(String, String, bool)>,
}

struct ExistingKey {
    id: String,
    provider_id: String,
    env_key: String,
    value: String,
    metadata: SecretMetadata,
}

/// Decrypted view of the vault, alive only while one review or apply runs.
struct Snapshot {
    groups: Vec<ExistingGroup>,
    keys: Vec<ExistingKey>,
    names: Vec<String>,
}

fn snapshot(vault: &VaultService) -> Result<Snapshot, String> {
    let metadata = vault.list_metadata().map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    let mut keys = Vec::new();
    for item in &metadata {
        match item.value_type.as_str() {
            "env_group" => groups.push(ExistingGroup {
                id: item.id.clone(),
                name: item.name.clone(),
                variables: vault
                    .group_values(&item.id, None)
                    .map_err(|e| e.to_string())?,
            }),
            "api_key" | "dev_secret" => {
                if let Ok(value) = vault.read_secret(&item.id) {
                    keys.push(ExistingKey {
                        id: item.id.clone(),
                        provider_id: item.provider_id.clone(),
                        env_key: item.env_key.clone(),
                        value,
                        metadata: item.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(Snapshot {
        groups,
        keys,
        names: metadata.into_iter().map(|item| item.name).collect(),
    })
}

fn same_name(left: &str, right: &str) -> bool {
    left.trim().to_lowercase() == right.trim().to_lowercase()
}

fn unique_name(base: &str, taken: &[String]) -> String {
    let mut candidate = base.to_owned();
    let mut counter = 2;
    while taken.iter().any(|name| same_name(name, &candidate)) {
        candidate = format!("{base} ({counter})");
        counter += 1;
    }
    candidate
}

// -------------------------------------------------------------------- review DTO

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DedupeInfo {
    /// `new` | `same` | `differs`
    pub state: &'static str,
    pub options: Vec<&'static str>,
    pub default: &'static str,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub missing: Vec<String>,
}

impl DedupeInfo {
    fn new_item() -> Self {
        Self {
            state: "new",
            options: Vec::new(),
            default: "create",
            added: Vec::new(),
            changed: Vec::new(),
            missing: Vec::new(),
        }
    }

    fn same() -> Self {
        Self {
            state: "same",
            options: vec!["skip"],
            default: "skip",
            ..Self::new_item()
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewVar {
    pub name: String,
    pub secret: bool,
    /// Number of distinct values found under this name (more than one is a duplicate).
    pub values: usize,
    /// Default choice among the values: the last one.
    pub pick: usize,
    pub conflict: bool,
    /// Item id of the API key this variable would be split into, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_item: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub id: String,
    /// `group` | `api_key`
    pub kind: &'static str,
    pub source: String,
    pub name: String,
    pub variables: Vec<ReviewVar>,
    pub has_notes: bool,
    pub dedupe: DedupeInfo,
    /// API keys: the group item the variables come from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_from: Option<String>,
    /// API keys: provider display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedView {
    pub name: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDto {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub items: Vec<ReviewItem>,
    pub skipped: Vec<SkippedView>,
}

fn secret_flag(var: &ScanVar, decision: &ItemDecision) -> bool {
    decision
        .secrets
        .get(&var.name)
        .copied()
        .unwrap_or(var.secret)
}

fn pick_value<'a>(var: &'a ScanVar, decision: &ItemDecision) -> &'a str {
    let index = decision
        .picks
        .get(&var.name)
        .copied()
        .filter(|index| *index < var.values.len())
        .unwrap_or_else(|| var.last_index());
    &var.values[index]
}

/// Variables a group ends up with: split-off provider keys leave, the rest keep the picked value.
fn final_group_variables(
    items: &[Item],
    group_index: usize,
    decisions: &ImportDecisions,
) -> Vec<(String, String, bool)> {
    let ItemData::Group(group) = &items[group_index].data else {
        return Vec::new();
    };
    let own = decisions.for_item(&items[group_index].id);
    let removed: HashSet<&str> = items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::ApiKey(key) if key.group_item == group_index => {
                let decision = decisions.for_item(&item.id);
                // A key that is unchecked or split off takes its variables out of the group.
                let stays = decision.include && decision.split == Some(false);
                (!stays).then_some(key.group_vars.iter().map(String::as_str))
            }
            _ => None,
        })
        .flatten()
        .collect();
    group
        .variables
        .iter()
        .filter(|var| !removed.contains(var.name.as_str()))
        .map(|var| {
            (
                var.name.clone(),
                pick_value(var, &own).to_owned(),
                secret_flag(var, &own),
            )
        })
        .collect()
}

struct GroupComparison {
    existing: Option<usize>,
    info: DedupeInfo,
}

fn compare_group(
    snapshot: &Snapshot,
    name: &str,
    variables: &[(String, String, bool)],
) -> GroupComparison {
    let Some(index) = snapshot
        .groups
        .iter()
        .position(|group| same_name(&group.name, name))
    else {
        return GroupComparison {
            existing: None,
            info: DedupeInfo::new_item(),
        };
    };
    let existing = &snapshot.groups[index];
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (var, value, _) in variables {
        match existing.variables.iter().find(|(n, _, _)| n == var) {
            None => added.push(var.clone()),
            Some((_, old, _)) if old != value => changed.push(var.clone()),
            Some(_) => {}
        }
    }
    let missing: Vec<String> = existing
        .variables
        .iter()
        .filter(|(n, _, _)| !variables.iter().any(|(var, _, _)| var == n))
        .map(|(n, _, _)| n.clone())
        .collect();
    let info = if added.is_empty() && changed.is_empty() && missing.is_empty() {
        DedupeInfo::same()
    } else {
        DedupeInfo {
            state: "differs",
            options: vec!["merge", "copy", "skip"],
            default: "merge",
            added,
            changed,
            missing,
        }
    };
    GroupComparison {
        existing: Some(index),
        info,
    }
}

enum KeyMatch {
    New,
    SameValue,
    /// Same provider and variable name, different value.
    Differs(usize),
}

fn compare_key(snapshot: &Snapshot, planned: &PlannedSecret) -> KeyMatch {
    if snapshot.keys.iter().any(|key| key.value == planned.value) {
        return KeyMatch::SameValue;
    }
    match snapshot.keys.iter().position(|key| {
        key.provider_id == planned.provider_id
            && !planned.env_key.is_empty()
            && key.env_key == planned.env_key
    }) {
        Some(index) => KeyMatch::Differs(index),
        None => KeyMatch::New,
    }
}

fn key_info(found: &KeyMatch) -> DedupeInfo {
    match found {
        KeyMatch::New => DedupeInfo::new_item(),
        KeyMatch::SameValue => DedupeInfo::same(),
        KeyMatch::Differs(_) => DedupeInfo {
            state: "differs",
            options: vec!["overwrite", "keep_both", "skip"],
            default: "keep_both",
            ..DedupeInfo::new_item()
        },
    }
}

/// Builds the value-free review shown to the user, with the default decisions applied.
pub fn review(
    vault: &VaultService,
    session: &Session,
    session_id: &str,
    registry: &[ProviderDefinition],
) -> Result<ReviewDto, String> {
    let snapshot = snapshot(vault)?;
    let defaults = ImportDecisions::default();
    let mut out = Vec::new();
    for (index, item) in session.items.iter().enumerate() {
        match &item.data {
            ItemData::Group(group) => {
                let final_vars = final_group_variables(&session.items, index, &defaults);
                let variables = group
                    .variables
                    .iter()
                    .map(|var| ReviewVar {
                        name: var.name.clone(),
                        secret: var.secret,
                        values: var.values.len(),
                        pick: var.last_index(),
                        conflict: var.conflict,
                        api_key_item: session.items.iter().find_map(|other| match &other.data {
                            ItemData::ApiKey(key)
                                if key.group_item == index
                                    && key.group_vars.contains(&var.name) =>
                            {
                                Some(other.id.clone())
                            }
                            _ => None,
                        }),
                    })
                    .collect();
                out.push(ReviewItem {
                    id: item.id.clone(),
                    kind: "group",
                    source: item.source.clone(),
                    name: group.name.clone(),
                    variables,
                    has_notes: !group.notes.is_empty(),
                    dedupe: compare_group(&snapshot, &group.name, &final_vars).info,
                    split_from: None,
                    provider: None,
                });
            }
            ItemData::ApiKey(key) => {
                let group_item = &session.items[key.group_item];
                let ItemData::Group(group) = &group_item.data else {
                    continue;
                };
                let variables = key
                    .group_vars
                    .iter()
                    .filter_map(|name| group.variables.iter().find(|var| &var.name == name))
                    .map(|var| ReviewVar {
                        name: var.name.clone(),
                        secret: var.secret,
                        values: 1,
                        pick: 0,
                        conflict: false,
                        api_key_item: None,
                    })
                    .collect();
                out.push(ReviewItem {
                    id: item.id.clone(),
                    kind: "api_key",
                    source: item.source.clone(),
                    name: key.planned.name.clone(),
                    variables,
                    has_notes: false,
                    dedupe: key_info(&compare_key(&snapshot, &key.planned)),
                    split_from: Some(group_item.id.clone()),
                    provider: registry
                        .iter()
                        .find(|provider| provider.id == key.planned.provider_id)
                        .map(|provider| provider.name.clone()),
                });
            }
        }
    }
    Ok(ReviewDto {
        session_id: session_id.to_owned(),
        request_id: session.request_id.clone(),
        items: out,
        skipped: session
            .skipped
            .iter()
            .map(|skipped| SkippedView {
                name: skipped.name.clone(),
                reason: skipped.reason,
            })
            .collect(),
    })
}

// ------------------------------------------------------------------------- apply

/// Names only: what the import did. This is also what the AI is told afterwards.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
    /// New provider API keys, which the UI validates one by one afterwards.
    pub api_key_ids: Vec<String>,
}

fn attributes_of(metadata: &SecretMetadata) -> secrethub_storage::SecretAttributes {
    secrethub_storage::SecretAttributes {
        value_type: metadata.value_type.clone(),
        category: metadata.category.clone(),
        scope: metadata.scope.clone(),
        favorite: metadata.favorite,
        archived: metadata.archived,
        model_id: metadata.model_id.clone(),
        model_env_key: metadata.model_env_key.clone(),
        endpoint_url: metadata.endpoint_url.clone(),
        endpoint_env_key: metadata.endpoint_env_key.clone(),
        subtitle: metadata.subtitle.clone(),
        dev_use: metadata.dev_use,
        ai_values: metadata.ai_values,
    }
}

fn choose<'a>(decision: &'a ItemDecision, info: &'a DedupeInfo) -> &'a str {
    decision
        .action
        .as_deref()
        .filter(|action| info.options.contains(action))
        .unwrap_or(info.default)
}

/// Writes the checked items. Re-compares with the vault first, so a stale review cannot create
/// duplicates; a failure after some items were written reports what was done in the error.
pub fn apply(
    vault: &VaultService,
    session: &Session,
    decisions: &ImportDecisions,
) -> Result<ImportOutcome, String> {
    let mut snapshot = snapshot(vault)?;
    let mut outcome = ImportOutcome::default();
    let mut taken_names = snapshot.names.clone();

    // Provider keys first, so group variables kept inside a group stay consistent.
    let mut new_keys: Vec<(PlannedSecret, usize)> = Vec::new();
    for (index, item) in session.items.iter().enumerate() {
        let ItemData::ApiKey(key) = &item.data else {
            continue;
        };
        let decision = decisions.for_item(&item.id);
        if !decision.include || decision.split == Some(false) {
            continue;
        }
        let found = compare_key(&snapshot, &key.planned);
        let info = key_info(&found);
        let action = choose(&decision, &info);
        match (found, action) {
            (KeyMatch::SameValue, _) | (_, "skip") => {
                outcome.skipped.push(key.planned.name.clone());
            }
            (KeyMatch::Differs(existing), "overwrite") => {
                let target = &snapshot.keys[existing];
                vault
                    .update_secret_with_attributes(
                        &target.id,
                        NewSecretInput {
                            name: target.metadata.name.clone(),
                            provider_id: target.metadata.provider_id.clone(),
                            env_key: target.metadata.env_key.clone(),
                            description: target.metadata.description.clone(),
                            tags: target.metadata.tags.clone(),
                            value: key.planned.value.clone(),
                        },
                        attributes_of(&target.metadata),
                    )
                    .map_err(|e| e.to_string())?;
                outcome.updated.push(target.metadata.name.clone());
            }
            _ => new_keys.push((key.planned.clone(), index)),
        }
    }
    let mut planned: Vec<PlannedSecret> = new_keys.into_iter().map(|(secret, _)| secret).collect();
    assign_names(&mut planned, &taken_names);
    for secret in planned {
        let attributes = crate::commands::import_attributes(&secret);
        let name = secret.name.clone();
        let id = vault
            .create_secret_with_attributes(
                NewSecretInput {
                    name: secret.name,
                    provider_id: secret.provider_id,
                    env_key: secret.env_key,
                    description: "Imported locally".to_owned(),
                    tags: vec!["imported".to_owned()],
                    value: secret.value,
                },
                attributes,
            )
            .map_err(|e| e.to_string())?;
        taken_names.push(name.clone());
        outcome.api_key_ids.push(id);
        outcome.added.push(name);
    }

    for (index, item) in session.items.iter().enumerate() {
        let ItemData::Group(group) = &item.data else {
            continue;
        };
        let decision = decisions.for_item(&item.id);
        if !decision.include {
            continue;
        }
        let variables = final_group_variables(&session.items, index, decisions);
        let comparison = compare_group(&snapshot, &group.name, &variables);
        let action = choose(&decision, &comparison.info).to_owned();
        let create = |name: String| -> Result<String, String> {
            vault
                .create_group(NewGroup {
                    name,
                    category: group.category.clone(),
                    variables: variables
                        .iter()
                        .map(|(name, value, secret)| NewVariable {
                            name: name.clone(),
                            value: value.clone(),
                            secret: *secret,
                        })
                        .collect(),
                    notes: group.notes.clone(),
                    tags: vec!["imported".to_owned()],
                    favorite: false,
                    ai_values: true,
                })
                .map_err(|e| e.to_string())
        };
        match (comparison.existing, comparison.info.state, action.as_str()) {
            (None, _, _) => {
                let id = create(group.name.clone())?;
                snapshot.groups.push(ExistingGroup {
                    id,
                    name: group.name.clone(),
                    variables: variables.clone(),
                });
                taken_names.push(group.name.clone());
                outcome.added.push(group.name.clone());
            }
            (Some(_), "same", _) | (Some(_), _, "skip") => {
                outcome.skipped.push(group.name.clone());
            }
            (Some(_), _, "copy") => {
                let name = unique_name(&group.name, &taken_names);
                create(name.clone())?;
                taken_names.push(name.clone());
                outcome.added.push(name);
            }
            (Some(existing), _, _) => {
                merge_into(vault, &snapshot.groups[existing], group, &variables)?;
                outcome.updated.push(snapshot.groups[existing].name.clone());
            }
        }
    }
    Ok(outcome)
}

/// Merge update: every stored variable stays unless the import carries a new value for it.
fn merge_into(
    vault: &VaultService,
    existing: &ExistingGroup,
    group: &GroupCand,
    imported: &[(String, String, bool)],
) -> Result<(), String> {
    let metadata = vault
        .list_metadata()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|item| item.id == existing.id)
        .ok_or_else(|| "group not found".to_owned())?;
    let mut variables: Vec<VariableUpdate> = Vec::new();
    for (name, value, secret) in &existing.variables {
        match imported.iter().find(|(n, _, _)| n == name) {
            Some((_, new_value, new_secret)) => variables.push(VariableUpdate {
                name: name.clone(),
                secret: *new_secret,
                value: Some(new_value.clone()),
                from: None,
            }),
            // Untouched: a stored secret keeps its value, a plain one must carry it again.
            None => variables.push(VariableUpdate {
                name: name.clone(),
                secret: *secret,
                value: (!*secret).then(|| value.clone()),
                from: None,
            }),
        }
    }
    for (name, value, secret) in imported {
        if !existing.variables.iter().any(|(n, _, _)| n == name) {
            variables.push(VariableUpdate {
                name: name.clone(),
                secret: *secret,
                value: Some(value.clone()),
                from: None,
            });
        }
    }
    let notes = if group.notes.is_empty() {
        None
    } else {
        let current = vault.group_notes(&existing.id).map_err(|e| e.to_string())?;
        if current.contains(&group.notes) {
            None
        } else if current.is_empty() {
            Some(group.notes.clone())
        } else {
            Some(format!("{current}\n\n{}", group.notes))
        }
    };
    vault
        .update_group(
            &existing.id,
            GroupUpdate {
                name: metadata.name.clone(),
                category: metadata.category.clone(),
                variables,
                notes,
                tags: metadata.tags.clone(),
                favorite: metadata.favorite,
                ai_values: metadata.ai_values,
            },
        )
        .map_err(|e| e.to_string())
}

/// Scans and builds a session. Paths must be absolute.
pub fn scan_session(
    paths: &[String],
    request_id: Option<String>,
    registry: &[ProviderDefinition],
) -> Result<Session, String> {
    let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let scanned = scan::scan_paths(&paths)?;
    if scanned.groups.is_empty() {
        return Err("nothing importable was found in the selected paths".to_owned());
    }
    Ok(Session {
        request_id,
        items: build_items(scanned.groups, registry),
        skipped: scanned.skipped,
    })
}

// ------------------------------------------------------------------ MCP queue files

const PREFIX: &str = "import-";
const KIND: &str = "import";
const MAX_PATHS: usize = 20;

/// `import-<id>.json`: paths only. The AI never reads the files; Desktop does, after review.
#[derive(Debug, Deserialize)]
pub struct PendingImport {
    #[serde(default)]
    pub version: u32,
    #[serde(alias = "requestId")]
    pub request_id: String,
    pub kind: String,
    pub paths: Vec<String>,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

impl PendingImport {
    /// Messages never echo paths or file content.
    fn validated(self, expected_id: &str) -> Result<Self, String> {
        if self.request_id != expected_id || !crate::queue::valid_id(&self.request_id, PREFIX) {
            return Err("invalid import request: request id".to_owned());
        }
        if self.kind != KIND || self.version > 1 {
            return Err("invalid import request: kind or version".to_owned());
        }
        let bad_path = |path: &String| {
            path.is_empty()
                || path.len() > 400
                || path.chars().any(char::is_control)
                || !std::path::Path::new(path).is_absolute()
        };
        if self.paths.is_empty() || self.paths.len() > MAX_PATHS || self.paths.iter().any(bad_path)
        {
            return Err("invalid import request: paths".to_owned());
        }
        Ok(self)
    }
}

pub fn read_pending(directory: &std::path::Path) -> Vec<PendingImport> {
    let mut requests: Vec<PendingImport> = crate::queue::pending_files(directory, PREFIX)
        .into_iter()
        .filter_map(|(id, path)| {
            let contents = std::fs::read_to_string(path).ok()?;
            serde_json::from_str::<PendingImport>(&contents)
                .ok()?
                .validated(&id)
                .ok()
        })
        .collect();
    requests.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    requests
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResultFile<'a> {
    request_id: &'a str,
    /// `done` | `declined` | `failed`
    status: &'static str,
    added: usize,
    updated: usize,
    skipped: usize,
    names: ResultNames<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResultError>,
    finished_at: String,
}

#[derive(Debug, Serialize)]
struct ResultNames<'a> {
    added: &'a [String],
    updated: &'a [String],
    skipped: &'a [String],
}

#[derive(Debug, Serialize)]
struct ResultError {
    code: &'static str,
}

fn write_result_file(
    results: &std::path::Path,
    request_id: &str,
    status: &'static str,
    outcome: &ImportOutcome,
    error: Option<&'static str>,
) -> Result<(), String> {
    let empty = ImportOutcome::default();
    let outcome = if status == "done" { outcome } else { &empty };
    let file = ImportResultFile {
        request_id,
        status,
        added: outcome.added.len(),
        updated: outcome.updated.len(),
        skipped: outcome.skipped.len(),
        names: ResultNames {
            added: &outcome.added,
            updated: &outcome.updated,
            skipped: &outcome.skipped,
        },
        error: error.map(|code| ResultError { code }),
        finished_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::queue::write_result(results, request_id, PREFIX, &file).map_err(|e| e.to_string())
}

fn directories(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    use tauri::Manager;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok((data.join("mcp-pending"), data.join("mcp-results")))
}

fn pending_file(directory: &std::path::Path, request_id: &str) -> Result<PathBuf, String> {
    if !crate::queue::valid_id(request_id, PREFIX) {
        return Err("invalid import request id".to_owned());
    }
    Ok(directory.join(format!("{request_id}.json")))
}

// -------------------------------------------------------------------- commands

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingImportView {
    pub request_id: String,
    pub paths: Vec<String>,
    pub created_at: String,
}

/// Queued AI import requests for the banner; needs no vault access.
#[tauri::command]
pub fn mcp_pending_import_requests(
    app: tauri::AppHandle,
) -> Result<Vec<PendingImportView>, String> {
    let (pending, _) = directories(&app)?;
    Ok(read_pending(&pending)
        .into_iter()
        .map(|request| PendingImportView {
            request_id: request.request_id,
            paths: request.paths,
            created_at: request.created_at,
        })
        .collect())
}

#[tauri::command]
pub fn mcp_import_decline(request_id: String, app: tauri::AppHandle) -> Result<(), String> {
    let (pending, results) = directories(&app)?;
    let path = pending_file(&pending, &request_id)?;
    write_result_file(
        &results,
        &request_id,
        "declined",
        &ImportOutcome::default(),
        None,
    )?;
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// Reads the paths locally and returns the value-free review. `request_id` links the review
/// to an AI request, which then gets the outcome when the user confirms.
#[tauri::command]
pub fn import_scan(
    paths: Vec<String>,
    request_id: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ReviewDto, String> {
    crate::commands::touch_activity(&state)?;
    let paths = match &request_id {
        Some(id) => {
            let (pending, _) = directories(&app)?;
            let request = read_pending(&pending)
                .into_iter()
                .find(|request| &request.request_id == id)
                .ok_or_else(|| "import request not found".to_owned())?;
            // The queued paths are authoritative; the web view cannot widen them.
            request.paths
        }
        None => paths,
    };
    let registry = crate::catalog::effective_registry();
    let session = scan_session(&paths, request_id, &registry)?;
    let guard = crate::commands::vault_from_state(&state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let dto = review(vault, &session, &session_id, &registry)?;
    state.imports.insert(session_id, session)?;
    Ok(dto)
}

#[tauri::command]
pub fn import_cancel(
    session_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    state.imports.take(&session_id).map(|_| ())
}

#[tauri::command]
pub fn import_apply(
    session_id: String,
    decisions: ImportDecisions,
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ImportOutcome, String> {
    crate::commands::touch_activity(&state)?;
    let session = state
        .imports
        .take(&session_id)?
        .ok_or_else(|| "import review expired; scan again".to_owned())?;
    let result = {
        let guard = crate::commands::vault_from_state(&state)?;
        let vault = guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        apply(vault, &session, &decisions)
    };
    if let Some(request_id) = &session.request_id {
        let (pending, results) = directories(&app)?;
        match &result {
            Ok(outcome) => write_result_file(&results, request_id, "done", outcome, None)?,
            Err(_) => write_result_file(
                &results,
                request_id,
                "failed",
                &ImportOutcome::default(),
                Some("import_failed"),
            )?,
        }
        let _ = std::fs::remove_file(pending_file(&pending, request_id)?);
    }
    if result.is_ok() {
        let guard = crate::commands::vault_from_state(&state)?;
        if let Some(vault) = guard.as_ref() {
            let _ = vault.record_action("import_applied", None, "{\"source\":\"desktop\"}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_storage::Database;
    use std::fs;
    use std::path::Path;

    const FAKE_KEY: &str =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----";
    const DASH: &str = "sk-fake-dashscope-value";
    const PASSWORD: &str = "fake-panel-password";

    fn vault(directory: &Path) -> VaultService {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        vault
    }

    fn registry() -> Vec<ProviderDefinition> {
        crate::catalog::effective_registry()
    }

    /// 服务器A: a .env with a duplicate key, a provider key and free text, plus a key file.
    fn fixture(root: &Path) -> PathBuf {
        let folder = root.join("服务器A");
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join(".env"),
            format!(
                "SSH_HOST=203.0.113.7\nBT_PANEL_PASSWORD={PASSWORD}\nSSH_PRIVATE_KEY=\"first-fake\"\nSSH_PRIVATE_KEY=\"{}\"\nDASHSCOPE_API_KEY={DASH}\nwords only line\n",
                FAKE_KEY.replace('\n', "\\n")
            ),
        )
        .unwrap();
        fs::write(folder.join("id_ed25519"), FAKE_KEY).unwrap();
        root.to_path_buf()
    }

    fn session_of(root: &Path) -> Session {
        scan_session(&[root.to_string_lossy().into_owned()], None, &registry()).unwrap()
    }

    fn item_id(session: &Session, kind: &str) -> String {
        session
            .items
            .iter()
            .find(|item| match &item.data {
                ItemData::Group(_) => kind == "group",
                ItemData::ApiKey(_) => kind == "api_key",
            })
            .unwrap()
            .id
            .clone()
    }

    fn decision(id: &str, patch: impl FnOnce(&mut ItemDecision)) -> ItemDecision {
        let mut decision = ItemDecision {
            id: id.to_owned(),
            include: true,
            ..Default::default()
        };
        patch(&mut decision);
        decision
    }

    fn decisions(items: Vec<ItemDecision>) -> ImportDecisions {
        ImportDecisions { items }
    }

    fn group_id_named(vault: &VaultService, name: &str) -> String {
        vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|item| item.value_type == "env_group" && item.name == name)
            .unwrap()
            .id
    }

    fn variable(name: &str, value: &str, secret: bool) -> NewVariable {
        NewVariable {
            name: name.into(),
            value: value.into(),
            secret,
        }
    }

    #[test]
    fn the_review_lists_names_and_flags_but_never_a_value() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let session = session_of(&fixture(root.path()));
        let dto = review(&vault, &session, "session-1", &registry()).unwrap();
        assert_eq!(dto.items.len(), 2);
        let group = &dto.items[0];
        assert_eq!((group.kind, group.name.as_str()), ("group", "服务器A"));
        assert_eq!(group.dedupe.state, "new");
        assert!(group.has_notes);
        let names: Vec<_> = group.variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "SSH_HOST",
                "BT_PANEL_PASSWORD",
                "SSH_PRIVATE_KEY",
                "DASHSCOPE_API_KEY"
            ]
        );
        let key = group
            .variables
            .iter()
            .find(|v| v.name == "SSH_PRIVATE_KEY")
            .unwrap();
        assert!(key.secret && key.values == 2 && key.pick == 1);
        let dashscope = group
            .variables
            .iter()
            .find(|v| v.name == "DASHSCOPE_API_KEY")
            .unwrap();
        assert_eq!(dashscope.api_key_item.as_deref(), Some("i1"));
        let api = &dto.items[1];
        assert_eq!(
            (api.kind, api.split_from.as_deref()),
            ("api_key", Some("i0"))
        );
        assert!(api.provider.is_some());
        let text = serde_json::to_string(&dto).unwrap();
        for value in [
            DASH,
            PASSWORD,
            "203.0.113.7",
            "FAKEFAKEFAKE",
            "words only line",
        ] {
            assert!(!text.contains(value), "{value} leaked");
        }
    }

    #[test]
    fn applying_with_defaults_creates_the_group_and_splits_the_provider_key() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let session = session_of(&fixture(root.path()));
        let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
        assert_eq!(outcome.added.len(), 2);
        assert!(outcome.added.contains(&"服务器A".to_owned()));
        assert_eq!(outcome.api_key_ids.len(), 1);
        let id = group_id_named(&vault, "服务器A");
        let rows = vault.group_variables(&id).unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        // The provider key left the group; the duplicate key keeps the last value.
        assert_eq!(names, ["SSH_HOST", "BT_PANEL_PASSWORD", "SSH_PRIVATE_KEY"]);
        assert_eq!(
            vault.group_variable_value(&id, "SSH_PRIVATE_KEY").unwrap(),
            FAKE_KEY
        );
        assert_eq!(
            vault
                .group_variable_value(&id, "BT_PANEL_PASSWORD")
                .unwrap(),
            PASSWORD
        );
        assert!(
            rows.iter()
                .find(|r| r.name == "BT_PANEL_PASSWORD")
                .unwrap()
                .secret
        );
        assert!(!rows.iter().find(|r| r.name == "SSH_HOST").unwrap().secret);
        assert!(vault
            .group_notes(&id)
            .unwrap()
            .contains("来源文件：.env\nwords only line"));
        assert_eq!(vault.read_secret(&outcome.api_key_ids[0]).unwrap(), DASH);
        let stored = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|item| item.id == outcome.api_key_ids[0])
            .unwrap();
        assert_eq!(stored.category, "ai");
        assert_eq!(stored.env_key, "DASHSCOPE_API_KEY");
        let text = serde_json::to_string(&outcome).unwrap();
        assert!(!text.contains(DASH) && !text.contains(PASSWORD));
    }

    #[test]
    fn user_choices_picks_secret_overrides_unchecking_and_keeping_keys_in_the_group() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        let session = session_of(&fixture(root.path()));
        let group = item_id(&session, "group");
        let api = item_id(&session, "api_key");
        let choices = decisions(vec![
            decision(&group, |d| {
                d.picks.insert("SSH_PRIVATE_KEY".into(), 0);
                d.secrets.insert("SSH_HOST".into(), true);
            }),
            decision(&api, |d| d.split = Some(false)),
        ]);
        let outcome = apply(&vault, &session, &choices).unwrap();
        assert_eq!(outcome.added, ["服务器A"]);
        assert!(outcome.api_key_ids.is_empty());
        let id = group_id_named(&vault, "服务器A");
        assert_eq!(
            vault.group_variable_value(&id, "SSH_PRIVATE_KEY").unwrap(),
            "first-fake"
        );
        // Kept inside the group, so the provider key is a (secret) variable there.
        assert_eq!(
            vault
                .group_variable_value(&id, "DASHSCOPE_API_KEY")
                .unwrap(),
            DASH
        );
        assert!(vault.group_variables(&id).unwrap()[0].secret);

        // Unchecking the group imports nothing; unchecking the key drops its variables too.
        let data2 = tempfile::tempdir().unwrap();
        let vault2 = self::vault(data2.path());
        let none = decisions(vec![
            decision(&group, |d| d.include = false),
            decision(&api, |d| d.include = false),
        ]);
        let outcome = apply(&vault2, &session, &none).unwrap();
        assert!(outcome.added.is_empty() && vault2.list_metadata().unwrap().is_empty());
        let only_group = decisions(vec![decision(&api, |d| d.include = false)]);
        apply(&vault2, &session, &only_group).unwrap();
        let id = group_id_named(&vault2, "服务器A");
        assert!(vault2
            .group_variable_value(&id, "DASHSCOPE_API_KEY")
            .is_err());
    }

    fn existing_group(vault: &VaultService, host: &str) -> String {
        vault
            .create_group(NewGroup {
                name: "服务器A".into(),
                category: "server".into(),
                variables: vec![
                    variable("SSH_HOST", host, false),
                    variable("BT_PANEL_PASSWORD", PASSWORD, true),
                    variable("SSH_PRIVATE_KEY", FAKE_KEY, true),
                    variable("EXTRA_ONLY_THERE", "kept", false),
                    variable("EXTRA_SECRET", "kept-secret", true),
                ],
                notes: "old notes".into(),
                tags: vec!["mine".into()],
                favorite: true,
                ai_values: false,
            })
            .unwrap()
    }

    #[test]
    fn same_name_groups_are_skipped_when_equal_and_offer_three_choices_when_different() {
        let root = tempfile::tempdir().unwrap();
        let session = session_of(&fixture(root.path()));
        let group = item_id(&session, "group");

        // Equal in every variable (nothing missing either) -> same.
        let data = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        vault
            .create_group(NewGroup {
                name: "服务器A".into(),
                category: "server".into(),
                variables: vec![
                    variable("SSH_HOST", "203.0.113.7", false),
                    variable("BT_PANEL_PASSWORD", PASSWORD, true),
                    variable("SSH_PRIVATE_KEY", FAKE_KEY, true),
                ],
                notes: String::new(),
                tags: Vec::new(),
                favorite: false,
                ai_values: true,
            })
            .unwrap();
        let dto = review(&vault, &session, "s", &registry()).unwrap();
        assert_eq!(
            (dto.items[0].dedupe.state, dto.items[0].dedupe.default),
            ("same", "skip")
        );
        let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
        assert_eq!(outcome.skipped, ["服务器A"]);
        assert_eq!(
            vault
                .list_metadata()
                .unwrap()
                .iter()
                .filter(|i| i.value_type == "env_group")
                .count(),
            1
        );

        // Different: lists added / changed / missing names and defaults to merge.
        let data = tempfile::tempdir().unwrap();
        let vault = self::vault(data.path());
        let id = existing_group(&vault, "198.51.100.9");
        let dto = review(&vault, &session, "s", &registry()).unwrap();
        let info = &dto.items[0].dedupe;
        assert_eq!((info.state, info.default), ("differs", "merge"));
        assert_eq!(info.options, ["merge", "copy", "skip"]);
        assert_eq!(info.changed, ["SSH_HOST"]);
        assert!(info.added.is_empty());
        assert_eq!(info.missing, ["EXTRA_ONLY_THERE", "EXTRA_SECRET"]);

        // Merge updates the stored group and keeps what the import does not mention.
        let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
        assert_eq!(outcome.updated, ["服务器A"]);
        assert_eq!(
            vault.group_variable_value(&id, "SSH_HOST").unwrap(),
            "203.0.113.7"
        );
        assert_eq!(
            vault.group_variable_value(&id, "EXTRA_ONLY_THERE").unwrap(),
            "kept"
        );
        assert_eq!(
            vault.group_variable_value(&id, "EXTRA_SECRET").unwrap(),
            "kept-secret"
        );
        let notes = vault.group_notes(&id).unwrap();
        assert!(notes.starts_with("old notes\n\n来源文件：.env"));
        assert!(
            !vault.group_public(&id).unwrap().ai_values,
            "group settings stay as the user set them"
        );
        let metadata = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|i| i.id == id)
            .unwrap();
        assert!(metadata.favorite && metadata.tags == ["mine"]);

        // Copy creates "name (2)"; skip leaves everything.
        let data = tempfile::tempdir().unwrap();
        let vault = self::vault(data.path());
        existing_group(&vault, "198.51.100.9");
        let copy = decisions(vec![decision(&group, |d| d.action = Some("copy".into()))]);
        let outcome = apply(&vault, &session, &copy).unwrap();
        assert!(outcome.added.contains(&"服务器A (2)".to_owned()));
        let skip = decisions(vec![decision(&group, |d| d.action = Some("skip".into()))]);
        let outcome = apply(&vault, &session, &skip).unwrap();
        assert!(outcome.skipped.contains(&"服务器A".to_owned()));
        // An action the state does not offer falls back to the default.
        let bogus = decisions(vec![decision(&group, |d| {
            d.action = Some("overwrite".into())
        })]);
        assert_eq!(
            apply(&vault, &session, &bogus).unwrap().updated,
            ["服务器A"]
        );
    }

    fn existing_key(vault: &VaultService, value: &str) -> String {
        vault
            .create_secret_with_attributes(
                NewSecretInput {
                    name: "Existing DashScope".into(),
                    provider_id: "qwen".into(),
                    env_key: "DASHSCOPE_API_KEY".into(),
                    description: String::new(),
                    tags: Vec::new(),
                    value: value.into(),
                },
                secrethub_storage::SecretAttributes::default(),
            )
            .unwrap()
    }

    #[test]
    fn provider_keys_are_skipped_when_equal_and_ask_when_only_the_value_differs() {
        let root = tempfile::tempdir().unwrap();
        let session = session_of(&fixture(root.path()));
        let api = item_id(&session, "api_key");

        let data = tempfile::tempdir().unwrap();
        let vault = vault(data.path());
        existing_key(&vault, DASH);
        let dto = review(&vault, &session, "s", &registry()).unwrap();
        assert_eq!(dto.items[1].dedupe.state, "same");
        let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
        assert!(outcome.api_key_ids.is_empty() && !outcome.skipped.is_empty());

        let data = tempfile::tempdir().unwrap();
        let vault = self::vault(data.path());
        let id = existing_key(&vault, "sk-older-different");
        let dto = review(&vault, &session, "s", &registry()).unwrap();
        let info = &dto.items[1].dedupe;
        assert_eq!((info.state, info.default), ("differs", "keep_both"));
        assert_eq!(info.options, ["overwrite", "keep_both", "skip"]);

        // Default keep_both: a second key with a numbered name.
        let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
        assert_eq!(outcome.api_key_ids.len(), 1);
        assert_ne!(outcome.api_key_ids[0], id);
        assert_eq!(vault.read_secret(&id).unwrap(), "sk-older-different");

        // Overwrite replaces the stored value and keeps the item.
        let data = tempfile::tempdir().unwrap();
        let vault = self::vault(data.path());
        let id = existing_key(&vault, "sk-older-different");
        let over = decisions(vec![decision(&api, |d| {
            d.action = Some("overwrite".into())
        })]);
        let outcome = apply(&vault, &session, &over).unwrap();
        assert_eq!(outcome.updated, ["Existing DashScope"]);
        assert_eq!(vault.read_secret(&id).unwrap(), DASH);
        assert_eq!(
            vault
                .list_metadata()
                .unwrap()
                .iter()
                .filter(|i| i.provider_id == "qwen")
                .count(),
            1
        );

        // Skip keeps the stored one.
        let data = tempfile::tempdir().unwrap();
        let vault = self::vault(data.path());
        let id = existing_key(&vault, "sk-older-different");
        let skip = decisions(vec![decision(&api, |d| d.action = Some("skip".into()))]);
        apply(&vault, &session, &skip).unwrap();
        assert_eq!(vault.read_secret(&id).unwrap(), "sk-older-different");
    }

    #[test]
    fn requests_are_validated_and_results_hold_names_and_counts_only() {
        let dir = tempfile::tempdir().unwrap();
        let abs = if cfg!(windows) {
            "C:\\\\work\\\\servers"
        } else {
            "/work/servers"
        };
        let good = format!(
            r#"{{"requestId":"import-1-aa","version":1,"kind":"import","paths":["{abs}"],"createdAt":"2026-11-09T00:00:00Z"}}"#
        );
        fs::write(dir.path().join("import-1-aa.json"), good).unwrap();
        for (name, body) in [
            (
                "import-2-bad.json",
                r#"{"requestId":"import-2-bad","kind":"import","paths":["relative"],"createdAt":"t"}"#,
            ),
            (
                "import-3-bad.json",
                r#"{"requestId":"import-3-bad","kind":"import","paths":[],"createdAt":"t"}"#,
            ),
            (
                "import-4-bad.json",
                r#"{"requestId":"import-OTHER","kind":"import","paths":["/a"],"createdAt":"t"}"#,
            ),
            (
                "import-5-bad.json",
                r#"{"requestId":"import-5-bad","kind":"sync","paths":["/a"],"createdAt":"t"}"#,
            ),
            (
                "sync-6.json",
                r#"{"requestId":"sync-6","kind":"import","paths":["/a"],"createdAt":"t"}"#,
            ),
        ] {
            fs::write(dir.path().join(name), body).unwrap();
        }
        let listed = read_pending(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].request_id, "import-1-aa");
        // Other readers never see import files.
        assert!(!crate::plan::valid_request_id("import-1-aa"));
        assert!(!crate::caps::valid_request_id("import-1-aa"));
        assert!(pending_file(dir.path(), "import-../x").is_err());

        let results = dir.path().join("mcp-results");
        let outcome = ImportOutcome {
            added: vec!["服务器A".into()],
            updated: vec!["Existing DashScope".into()],
            skipped: vec!["充值".into()],
            api_key_ids: vec!["secret-1".into()],
        };
        write_result_file(&results, "import-1-aa", "done", &outcome, None).unwrap();
        let text = fs::read_to_string(results.join("import-1-aa.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["status"], "done");
        assert_eq!(
            (
                value["added"].as_u64(),
                value["updated"].as_u64(),
                value["skipped"].as_u64()
            ),
            (Some(1), Some(1), Some(1))
        );
        assert_eq!(value["names"]["added"][0], "服务器A");
        assert!(
            !text.contains("secret-1"),
            "ids of new keys stay out of the result"
        );
        write_result_file(
            &results,
            "import-2-x",
            "failed",
            &outcome,
            Some("import_failed"),
        )
        .unwrap();
        let failed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(results.join("import-2-x.json")).unwrap())
                .unwrap();
        assert_eq!(
            (
                failed["status"].as_str(),
                failed["error"]["code"].as_str(),
                failed["added"].as_u64()
            ),
            (Some("failed"), Some("import_failed"), Some(0))
        );
    }

    #[test]
    fn scanning_nothing_usable_fails_and_old_sessions_are_dropped() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("empty.txt"), "").unwrap();
        let error = scan_session(
            &[root.path().to_string_lossy().into_owned()],
            None,
            &registry(),
        )
        .err()
        .unwrap();
        assert!(error.contains("nothing importable"));
        let sessions = Sessions::default();
        for index in 0..6 {
            sessions
                .insert(
                    format!("session-{index}"),
                    Session {
                        request_id: None,
                        items: Vec::new(),
                        skipped: Vec::new(),
                    },
                )
                .unwrap();
        }
        assert!(
            sessions.take("session-0").unwrap().is_none(),
            "oldest reviews are dropped"
        );
        assert!(sessions.take("session-5").unwrap().is_some());
        assert!(sessions.take("session-5").unwrap().is_none());
    }
}
