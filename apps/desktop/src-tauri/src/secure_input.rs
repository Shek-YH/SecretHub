//! Secure input forms. An AI asks for a form (`input-*.json`, written by MCP) instead of having
//! the user paste a password or API key into chat. Desktop shows the form, verifies what the user
//! typed (login endpoint or the provider's `/models`) and only then stores it in the vault as one
//! undoable import batch.
//!
//! Plaintext lives only in the form, in this process's memory during `mcp_input_submit` and in
//! the vault. It never reaches a queue file, a result file, the audit trail, a log or an error:
//! `InputError` carries a code, an HTTP status and a template field key, nothing else.

use crate::{catalog, commands, import::PlannedSecret, import_flow, model_sync, queue, AppState};
use chrono::{DateTime, Utc};
use reqwest::{redirect::Policy, Client};
use secrethub_core::groups::{valid_category, valid_variable_name};
use secrethub_core::{NewGroup, NewItem, NewSecretInput, NewVariable, VaultService};
use secrethub_providers::{ModelsEndpoint, Protocol, ProviderDefinition};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    time::Duration,
};
use tauri::{AppHandle, Manager, State};
use url::{Host, Url};

const PREFIX: &str = "input-";
const KIND: &str = "request_input";
const EXPIRY_HOURS: i64 = 24;
const MAX_VALUE_CHARS: usize = 4096;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const LOGIN_KEYS: [&str; 5] = ["website", "username", "password", "totp", "notes"];

// ----------------------------------------------------------------- template

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    title: String,
    #[serde(default)]
    hint: Option<String>,
    item_type: String,
    name: String,
    #[serde(default)]
    provider_id: Option<String>,
    #[serde(default)]
    category: Option<String>,
    fields: Vec<Field>,
    #[serde(default)]
    verify: Option<Verify>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Field {
    key: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verify {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    body_user: Option<String>,
    #[serde(default)]
    body_pass: Option<String>,
    #[serde(default)]
    ok_field: Option<String>,
    #[serde(default)]
    auth: Option<String>,
}

impl Template {
    fn verify_kind(&self) -> &str {
        self.verify.as_ref().map_or("none", |verify| &verify.kind)
    }

    fn field_with_role(&self, role: &str) -> Option<&Field> {
        self.fields
            .iter()
            .find(|field| field.role.as_deref() == Some(role))
    }

    fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == key)
    }

    fn llm_auth(&self) -> &str {
        self.verify
            .as_ref()
            .and_then(|verify| verify.auth.as_deref())
            .unwrap_or("bearer")
    }
}

fn within(text: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&text.chars().count())
}

fn body_name_ok(name: &str) -> bool {
    within(name, 1, 64)
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

/// https, or http for loopback only; never credentials inside the URL.
fn checked_url(text: &str) -> Option<Url> {
    let url = Url::parse(text.trim()).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let loopback = match url.host()? {
        Host::Domain(name) => name == "localhost",
        Host::Ipv4(address) => address.is_loopback() && address.octets() == [127, 0, 0, 1],
        Host::Ipv6(address) => address.is_loopback(),
    };
    match url.scheme() {
        "https" => Some(url),
        "http" if loopback => Some(url),
        _ => None,
    }
}

/// The reason code of the first rule the template breaks (design: secure-input spec section 1).
pub fn validate_template(
    template: &Template,
    providers: &[ProviderDefinition],
) -> Result<(), &'static str> {
    const BAD_SHAPE: &str = "bad_shape";
    let item_type = template.item_type.as_str();
    if !within(&template.title, 1, 100)
        || !within(&template.name, 1, 100)
        || template
            .hint
            .as_deref()
            .is_some_and(|hint| !within(hint, 0, 500))
        || !matches!(item_type, "login" | "api_key" | "env_group")
        || template
            .provider_id
            .as_deref()
            .is_some_and(|id| !within(id, 1, 100))
        || !(1..=20).contains(&template.fields.len())
    {
        return Err(BAD_SHAPE);
    }
    for field in &template.fields {
        let options_ok = field.options.as_ref().is_none_or(|options| {
            options.len() <= 200 && options.iter().all(|option| within(option, 0, 200))
        });
        if !within(&field.key, 1, 64)
            || !within(&field.label, 1, 100)
            || !matches!(field.kind.as_str(), "text" | "password" | "select")
            || field
                .value
                .as_deref()
                .is_some_and(|value| !within(value, 0, 2000))
            || !options_ok
            || field
                .role
                .as_deref()
                .is_some_and(|role| !matches!(role, "api_key" | "base_url" | "model"))
        {
            return Err(BAD_SHAPE);
        }
    }
    if let Some(verify) = &template.verify {
        let names_ok = [&verify.body_user, &verify.body_pass, &verify.ok_field]
            .into_iter()
            .flatten()
            .all(|name| body_name_ok(name));
        let shape_ok = match verify.kind.as_str() {
            "none" => true,
            "login" => {
                names_ok
                    && verify
                        .url
                        .as_deref()
                        .is_some_and(|url| within(url, 1, 2000))
            }
            "llm" => matches!(verify.auth.as_deref(), Some("bearer" | "x-api-key")),
            _ => false,
        };
        if !shape_ok {
            return Err(BAD_SHAPE);
        }
    }

    let mut keys = HashSet::new();
    if !template
        .fields
        .iter()
        .all(|field| keys.insert(field.key.as_str()))
    {
        return Err("duplicate_key");
    }
    if template.fields.iter().any(|field| {
        field.kind == "password"
            && field
                .value
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }) {
        return Err("prefilled_secret");
    }
    if template
        .fields
        .iter()
        .any(|field| field.kind != "select" && field.options.is_some())
    {
        return Err("options_not_select");
    }
    let keys_ok = template.fields.iter().all(|field| match item_type {
        "login" => LOGIN_KEYS.contains(&field.key.as_str()),
        _ => valid_variable_name(&field.key),
    });
    if !keys_ok {
        return Err("bad_key");
    }
    if item_type == "login" && template.field("username").is_none() {
        return Err("login_needs_username");
    }
    if item_type == "login" && template.fields.iter().any(|field| field.role.is_some()) {
        return Err("role_not_allowed");
    }
    let mut roles = HashSet::new();
    if !template
        .fields
        .iter()
        .filter_map(|field| field.role.as_deref())
        .all(|role| roles.insert(role))
    {
        return Err("duplicate_role");
    }
    if item_type == "api_key" {
        let key_ok = template
            .field_with_role("api_key")
            .is_some_and(|field| field.kind == "password");
        let others_ok = template
            .fields
            .iter()
            .all(|field| match field.role.as_deref() {
                Some("api_key") => true,
                Some("base_url") => field.kind == "text",
                Some("model") => matches!(field.kind.as_str(), "text" | "select"),
                _ => false,
            });
        if !key_ok || !others_ok {
            return Err("api_key_fields");
        }
    }
    let provider_ok = match template.provider_id.as_deref() {
        Some(id) => providers.iter().any(|provider| provider.id == id),
        None => item_type != "api_key",
    };
    if !provider_ok {
        return Err("unknown_provider");
    }
    let verify_ok = match template.verify_kind() {
        "login" => {
            item_type == "login"
                && template.field("username").is_some()
                && template.field("password").is_some()
        }
        "llm" => {
            template.field_with_role("api_key").is_some()
                && template.field_with_role("base_url").is_some()
        }
        _ => true,
    };
    if !verify_ok {
        return Err("verify_mismatch");
    }
    if let Some(verify) = template
        .verify
        .as_ref()
        .filter(|verify| verify.kind == "login")
    {
        if verify.url.as_deref().and_then(checked_url).is_none() {
            return Err("bad_url");
        }
    }
    Ok(())
}

/// Value a field starts with: the AI's prefill, or the provider's address for an empty base URL.
fn initial_value(field: &Field, template: &Template, providers: &[ProviderDefinition]) -> String {
    if field.kind == "password" {
        return String::new();
    }
    let value = field.value.clone().unwrap_or_default();
    if !value.is_empty() || field.role.as_deref() != Some("base_url") {
        return value;
    }
    template
        .provider_id
        .as_deref()
        .and_then(|id| providers.iter().find(|provider| provider.id == id))
        .map(|provider| provider.base_url.clone())
        .unwrap_or_default()
}

// --------------------------------------------------------------------- views

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldView {
    key: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    value: String,
    options: Vec<String>,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyView {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRequestView {
    request_id: String,
    created_at: String,
    title: String,
    hint: String,
    item_type: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_name: Option<String>,
    fields: Vec<FieldView>,
    verify: VerifyView,
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

fn build_view(
    request_id: &str,
    created_at: &str,
    template: &Template,
    providers: &[ProviderDefinition],
) -> InputRequestView {
    let fields: Vec<FieldView> = template
        .fields
        .iter()
        .map(|field| FieldView {
            key: field.key.clone(),
            label: field.label.clone(),
            kind: field.kind.clone(),
            value: initial_value(field, template, providers),
            options: field.options.clone().unwrap_or_default(),
            required: field.required,
            role: field.role.clone(),
        })
        .collect();
    let verify = match template.verify_kind() {
        "login" => VerifyView {
            kind: "login".to_owned(),
            host: template
                .verify
                .as_ref()
                .and_then(|verify| verify.url.as_deref())
                .and_then(host_of),
            auth: None,
        },
        "llm" => VerifyView {
            kind: "llm".to_owned(),
            host: fields
                .iter()
                .find(|field| field.role.as_deref() == Some("base_url"))
                .and_then(|field| host_of(&field.value)),
            auth: Some(template.llm_auth().to_owned()),
        },
        _ => VerifyView {
            kind: "none".to_owned(),
            host: None,
            auth: None,
        },
    };
    let provider = template
        .provider_id
        .as_deref()
        .and_then(|id| providers.iter().find(|provider| provider.id == id));
    InputRequestView {
        request_id: request_id.to_owned(),
        created_at: created_at.to_owned(),
        title: template.title.clone(),
        hint: template.hint.clone().unwrap_or_default(),
        item_type: template.item_type.clone(),
        name: template.name.clone(),
        provider_id: provider.map(|provider| provider.id.clone()),
        provider_name: provider.map(|provider| provider.name.clone()),
        fields,
        verify,
    }
}

// --------------------------------------------------------------------- errors

/// What the front end learns about a failed attempt. No message, no response body, no value.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputError {
    /// `locked` | `not_found` | `invalid_request` | `missing_field` | `invalid_value` |
    /// `bad_url` | `network` | `unauthorized` | `http_error` | `not_ok` | `bad_response` |
    /// `write_failed`
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Template field key the error is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl InputError {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            http_status: None,
            field: None,
        }
    }

    fn with_field(code: &'static str, field: &str) -> Self {
        Self {
            field: Some(field.to_owned()),
            ..Self::new(code)
        }
    }

    fn from_status(status: u16) -> Self {
        let code = if matches!(status, 401 | 403) {
            "unauthorized"
        } else {
            "http_error"
        };
        Self {
            http_status: Some(status),
            ..Self::new(code)
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputSaved {
    pub item_id: String,
    pub item_type: String,
}

// ------------------------------------------------------------------ queue files

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    #[serde(default)]
    version: u32,
    request_id: String,
    kind: String,
    created_at: String,
    template: Template,
}

impl Pending {
    fn created(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.created_at)
            .ok()
            .map(|time| time.with_timezone(&Utc))
    }
}

/// Reads and validates one pending request; `None` when the file is not a valid request.
fn parse_pending(text: &str, id: &str, providers: &[ProviderDefinition]) -> Option<Pending> {
    let pending: Pending = serde_json::from_str(text).ok()?;
    let valid = pending.request_id == id
        && queue::valid_id(id, PREFIX)
        && pending.kind == KIND
        && pending.version <= 1
        && pending.created().is_some()
        && validate_template(&pending.template, providers).is_ok();
    valid.then_some(pending)
}

fn read_pending(
    directory: &Path,
    id: &str,
    providers: &[ProviderDefinition],
) -> Result<Pending, InputError> {
    if !queue::valid_id(id, PREFIX) {
        return Err(InputError::new("invalid_request"));
    }
    let text = fs::read_to_string(directory.join(format!("{id}.json")))
        .map_err(|_| InputError::new("not_found"))?;
    parse_pending(&text, id, providers).ok_or_else(|| InputError::new("invalid_request"))
}

enum Outcome<'a> {
    Saved {
        item_id: &'a str,
        item_type: &'a str,
    },
    Cancelled,
    Failed(&'static str),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultError {
    code: &'static str,
}

/// Only these fields ever reach a result file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InputResult<'a> {
    request_id: &'a str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResultError>,
    updated_at: String,
}

/// Writes the result file first, then removes the pending file.
fn finish_in(pending: &Path, results: &Path, id: &str, outcome: Outcome<'_>) -> Result<(), String> {
    let mut result = InputResult {
        request_id: id,
        status: "cancelled",
        item_id: None,
        item_type: None,
        error: None,
        updated_at: Utc::now().to_rfc3339(),
    };
    match outcome {
        Outcome::Saved { item_id, item_type } => {
            result.status = "saved";
            result.item_id = Some(item_id);
            result.item_type = Some(item_type);
        }
        Outcome::Cancelled => {}
        Outcome::Failed(code) => {
            result.status = "failed";
            result.error = Some(ResultError { code });
        }
    }
    queue::write_result(results, id, PREFIX, &result).map_err(|error| error.to_string())?;
    match fs::remove_file(pending.join(format!("{id}.json"))) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

/// Views of every valid, fresh request, oldest first. Broken or expired files get a `failed`
/// result and are removed.
fn scan_requests(
    pending: &Path,
    results: &Path,
    providers: &[ProviderDefinition],
    now: DateTime<Utc>,
) -> Vec<InputRequestView> {
    let mut found: Vec<(DateTime<Utc>, InputRequestView)> = Vec::new();
    for (id, path) in queue::pending_files(pending, PREFIX) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(request) = parse_pending(&text, &id, providers) else {
            let _ = finish_in(pending, results, &id, Outcome::Failed("invalid_template"));
            continue;
        };
        let Some(created) = request.created() else {
            continue;
        };
        if now.signed_duration_since(created).num_hours() >= EXPIRY_HOURS {
            let _ = finish_in(pending, results, &id, Outcome::Failed("expired"));
            continue;
        }
        found.push((
            created,
            build_view(&id, &request.created_at, &request.template, providers),
        ));
    }
    found.sort_by_key(|(created, _)| *created);
    found.into_iter().map(|(_, view)| view).collect()
}

// -------------------------------------------------------------------- values

/// Non-empty submitted values by template key, after the checks of spec section 4.3.
fn check_values(
    template: &Template,
    values: &HashMap<String, String>,
) -> Result<HashMap<String, String>, InputError> {
    if values.keys().any(|key| template.field(key).is_none()) {
        return Err(InputError::new("invalid_value"));
    }
    let mut checked = HashMap::new();
    for field in &template.fields {
        let raw = values.get(&field.key).map_or("", String::as_str);
        // Passwords and keys are kept exactly as typed; everything else loses stray whitespace.
        let value = if field.kind == "password" {
            raw
        } else {
            raw.trim()
        };
        if value.chars().count() > MAX_VALUE_CHARS {
            return Err(InputError::with_field("invalid_value", &field.key));
        }
        if value.is_empty() {
            if field.required {
                return Err(InputError::with_field("missing_field", &field.key));
            }
            continue;
        }
        if field.role.as_deref() == Some("base_url") && checked_url(value).is_none() {
            return Err(InputError::with_field("bad_url", &field.key));
        }
        checked.insert(field.key.clone(), value.to_owned());
    }
    Ok(checked)
}

// ------------------------------------------------------------- verification

fn client() -> Result<Client, InputError> {
    Client::builder()
        .redirect(Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| InputError::new("network"))
}

fn truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        serde_json::Value::String(text) => !matches!(text.trim(), "" | "0" | "false"),
        serde_json::Value::Null => false,
        _ => true,
    }
}

/// POSTs the credentials as JSON to the template's login URL. The response body is only read
/// for `okField` and never copied into an error.
pub async fn verify_login(
    verify: &Verify,
    username: &str,
    password: &str,
) -> Result<(), InputError> {
    let url = verify
        .url
        .as_deref()
        .and_then(checked_url)
        .ok_or_else(|| InputError::new("bad_url"))?;
    let mut body = serde_json::Map::new();
    body.insert(
        verify
            .body_user
            .clone()
            .unwrap_or_else(|| "username".to_owned()),
        username.into(),
    );
    body.insert(
        verify
            .body_pass
            .clone()
            .unwrap_or_else(|| "password".to_owned()),
        password.into(),
    );
    let mut response = client()?
        .post(url)
        .header("content-type", "application/json")
        .body(serde_json::Value::Object(body).to_string())
        .send()
        .await
        .map_err(|_| InputError::new("network"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(InputError::from_status(status.as_u16()));
    }
    let Some(ok_field) = verify.ok_field.as_deref() else {
        return Ok(());
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| InputError::new("network"))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(InputError::new("bad_response"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| InputError::new("bad_response"))?;
    if json.get(ok_field).is_some_and(truthy) {
        Ok(())
    } else {
        Err(InputError::new("not_ok"))
    }
}

/// `{base}/models` with the given auth style; sorted, de-duplicated model ids.
pub async fn fetch_models(
    base_url: &str,
    api_key: &str,
    auth: &str,
) -> Result<Vec<String>, InputError> {
    let base = checked_url(base_url).ok_or_else(|| InputError::new("bad_url"))?;
    let endpoint = ModelsEndpoint {
        url: format!("{}/models", base.as_str().trim_end_matches('/')),
        protocol: if auth == "x-api-key" {
            Protocol::Anthropic
        } else {
            Protocol::Openai
        },
        is_fallback: false,
        lists_models: true,
    };
    let fetched = model_sync::fetch(&endpoint, api_key, true).await;
    let Some(status) = fetched.http_status else {
        return Err(InputError::new("network"));
    };
    if !(200..300).contains(&status) {
        return Err(InputError::from_status(status));
    }
    let models = fetched
        .models
        .ok_or_else(|| InputError::new("bad_response"))?;
    let mut ids: Vec<String> = models.into_iter().map(|model| model.id).collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn verify_submission(
    template: &Template,
    values: &HashMap<String, String>,
) -> Result<(), InputError> {
    let value = |key: &str| values.get(key).map_or("", String::as_str);
    match template.verify.as_ref() {
        Some(verify) if verify.kind == "login" => {
            verify_login(verify, value("username"), value("password")).await
        }
        Some(verify) if verify.kind == "llm" => {
            let (Some(key), Some(base)) = (
                template.field_with_role("api_key"),
                template.field_with_role("base_url"),
            ) else {
                return Err(InputError::new("invalid_request"));
            };
            if value(&base.key).is_empty() {
                return Err(InputError::with_field("missing_field", &base.key));
            }
            if value(&key.key).is_empty() {
                return Err(InputError::with_field("missing_field", &key.key));
            }
            fetch_models(value(&base.key), value(&key.key), template.llm_auth())
                .await
                .map(|_| ())
        }
        _ => Ok(()),
    }
}

// ------------------------------------------------------------------- storing

fn begin_and<T>(
    vault: &VaultService,
    write: impl FnOnce(&str) -> Result<(T, [i64; 4]), String>,
) -> Result<(T, String), String> {
    let batch = vault.begin_import_batch().map_err(|e| e.to_string())?;
    match write(&batch) {
        Ok((value, counts)) => {
            vault
                .finish_import_batch(&batch, counts)
                .map_err(|e| e.to_string())?;
            Ok((value, batch))
        }
        Err(error) => {
            let _ = vault.dismiss_import_batch(&batch);
            Err(error)
        }
    }
}

/// Writes the checked values as one import batch and returns `(item id, batch id)`.
pub fn store(
    vault: &VaultService,
    template: &Template,
    values: &HashMap<String, String>,
    registry: &[ProviderDefinition],
) -> Result<(String, String), String> {
    let taken = import_flow::snapshot(vault)?.names;
    let name = import_flow::unique_name(template.name.trim(), &taken);
    let present = |field: &Field| values.get(&field.key).filter(|value| !value.is_empty());
    match template.item_type.as_str() {
        "login" => {
            let fields: BTreeMap<String, String> = template
                .fields
                .iter()
                .filter_map(|field| Some((field.key.clone(), present(field)?.clone())))
                .collect();
            begin_and(vault, |batch| {
                let id = vault
                    .create_item(NewItem {
                        item_type: "login".to_owned(),
                        name,
                        fields,
                        tags: Vec::new(),
                        favorite: false,
                        dev_use: false,
                        env_key: String::new(),
                    })
                    .map_err(|e| e.to_string())?;
                vault
                    .record_batch_entry(batch, &id, None)
                    .map_err(|e| e.to_string())?;
                Ok((id, [0, 0, 0, 1]))
            })
        }
        "api_key" => {
            let provider = template
                .provider_id
                .as_deref()
                .and_then(|id| registry.iter().find(|provider| provider.id == id))
                .ok_or("unknown provider")?;
            let key_field = template
                .field_with_role("api_key")
                .ok_or("missing key field")?;
            let key = present(key_field).ok_or("missing key value")?;
            let role_value = |role: &str| {
                template
                    .field_with_role(role)
                    .and_then(|field| present(field).map(|value| (field.key.clone(), value)))
            };
            let preset = crate::capture::build_preset(provider, "");
            let (endpoint_env_key, endpoint_url) = role_value("base_url").map_or(
                (preset.endpoint_env_key.clone(), preset.endpoint_url.clone()),
                |(key, value)| (key, value.clone()),
            );
            let (model_env_key, model_id) = role_value("model").map_or(
                (preset.model_env_key.clone(), preset.model_id.clone()),
                |(key, value)| (key, value.clone()),
            );
            let planned = PlannedSecret {
                name,
                provider_id: provider.id.clone(),
                env_key: key_field.key.clone(),
                value: key.clone(),
                provider_credential: true,
                endpoint_url,
                endpoint_env_key,
                model_id,
                model_env_key,
                source_keys: Vec::new(),
            };
            begin_and(vault, |batch| {
                let id = vault
                    .create_secret_with_attributes(
                        NewSecretInput {
                            name: planned.name.clone(),
                            provider_id: planned.provider_id.clone(),
                            env_key: planned.env_key.clone(),
                            description: "Added via AI input form".to_owned(),
                            tags: Vec::new(),
                            value: planned.value.clone(),
                        },
                        commands::import_attributes(&planned),
                    )
                    .map_err(|e| e.to_string())?;
                vault
                    .record_batch_entry(batch, &id, None)
                    .map_err(|e| e.to_string())?;
                Ok((id, [0, 0, 0, 1]))
            })
        }
        "env_group" => {
            let category = template
                .category
                .as_deref()
                .filter(|category| valid_category(category))
                .unwrap_or("other")
                .to_owned();
            let variables: Vec<NewVariable> = template
                .fields
                .iter()
                .filter_map(|field| {
                    Some(NewVariable {
                        name: field.key.clone(),
                        value: present(field)?.clone(),
                        secret: field.kind == "password"
                            || field.role.as_deref() == Some("api_key"),
                        description: field.label.clone(),
                    })
                })
                .collect();
            let names: Vec<String> = variables
                .iter()
                .map(|variable| variable.name.clone())
                .collect();
            begin_and(vault, |batch| {
                let id = vault
                    .create_group(NewGroup {
                        name,
                        category,
                        variables,
                        notes: String::new(),
                        tags: Vec::new(),
                        favorite: false,
                        ai_values: true,
                    })
                    .map_err(|e| e.to_string())?;
                vault
                    .record_batch_entry(batch, &id, None)
                    .map_err(|e| e.to_string())?;
                for variable in &names {
                    vault
                        .record_batch_entry(batch, &id, Some(variable))
                        .map_err(|e| e.to_string())?;
                }
                Ok((id, [1, 0, 0, 0]))
            })
        }
        _ => Err("unsupported item type".to_owned()),
    }
}

// ------------------------------------------------------------------ commands

fn locked_or_failed(message: &str) -> InputError {
    InputError::new(if message.contains("auto-locked") {
        "locked"
    } else {
        "write_failed"
    })
}

/// Runs `action` with the unlocked vault; a locked or just auto-locked vault is `locked`.
fn with_unlocked<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&VaultService) -> Result<T, InputError>,
) -> Result<T, InputError> {
    commands::touch_activity(state).map_err(|message| locked_or_failed(&message))?;
    let guard = commands::vault_from_state(state).map_err(|message| locked_or_failed(&message))?;
    match guard.as_ref() {
        Some(vault) if vault.is_unlocked() => action(vault),
        _ => Err(InputError::new("locked")),
    }
}

fn directories(app: &AppHandle) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    crate::capture::directories(app)
}

#[tauri::command]
pub fn mcp_pending_input_requests(app: AppHandle) -> Result<Vec<InputRequestView>, String> {
    let (pending, results) = directories(&app)?;
    Ok(scan_requests(
        &pending,
        &results,
        &catalog::effective_registry(),
        Utc::now(),
    ))
}

#[tauri::command]
pub async fn mcp_input_fetch_models(
    request_id: String,
    values: HashMap<String, String>,
    app: AppHandle,
) -> Result<Vec<String>, InputError> {
    let (pending, _) = directories(&app).map_err(|_| InputError::new("write_failed"))?;
    let request = read_pending(&pending, &request_id, &catalog::effective_registry())?;
    let template = request.template;
    let (Some(key), Some(base)) = (
        template.field_with_role("api_key"),
        template.field_with_role("base_url"),
    ) else {
        return Err(InputError::new("invalid_request"));
    };
    let value = |field: &Field| {
        values
            .get(&field.key)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .filter(|value| value.chars().count() <= MAX_VALUE_CHARS)
            .ok_or_else(|| InputError::with_field("missing_field", &field.key))
    };
    fetch_models(&value(base)?, &value(key)?, template.llm_auth()).await
}

#[tauri::command]
pub async fn mcp_input_submit(
    request_id: String,
    values: HashMap<String, String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<InputSaved, InputError> {
    let registry = catalog::effective_registry();
    let (pending, results) = directories(&app).map_err(|_| InputError::new("write_failed"))?;
    let template = read_pending(&pending, &request_id, &registry)?.template;
    with_unlocked(&state, |_| Ok(()))?;
    let values = check_values(&template, &values)?;
    verify_submission(&template, &values).await?;
    // The vault may have locked while the request was in flight.
    let (item_id, _batch) = with_unlocked(&state, |vault| {
        let (id, batch) = store(vault, &template, &values, &registry)
            .map_err(|_| InputError::new("write_failed"))?;
        let _ = vault.record_action(
            "secure_input_saved",
            Some(&id),
            &serde_json::json!({"source": "mcp", "itemType": template.item_type}).to_string(),
        );
        Ok((id, batch))
    })?;
    let _ = finish_in(
        &pending,
        &results,
        &request_id,
        Outcome::Saved {
            item_id: &item_id,
            item_type: &template.item_type,
        },
    );
    if template.item_type == "api_key" {
        let app = app.clone();
        let id = item_id.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let _ = commands::check_secret(&id, &state, true).await;
        });
    }
    Ok(InputSaved {
        item_id,
        item_type: template.item_type,
    })
}

#[tauri::command]
pub fn mcp_input_cancel(request_id: String, app: AppHandle) -> Result<(), String> {
    if !queue::valid_id(&request_id, PREFIX) {
        return Err("invalid request id".to_owned());
    }
    let (pending, results) = directories(&app)?;
    finish_in(&pending, &results, &request_id, Outcome::Cancelled)
}

#[tauri::command]
pub fn focus_main_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
        .ok_or_else(|| "no window".to_owned())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_storage::Database;
    use serde_json::{json, Value};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    const PASSWORD: &str = "fake-test-pass-0000";
    const API_KEY: &str = "sk-fake-test-key-0000";

    fn registry() -> Vec<ProviderDefinition> {
        catalog::effective_registry()
    }

    fn provider() -> ProviderDefinition {
        registry()
            .into_iter()
            .find(|p| p.model_env_key.is_some() && !p.env_keys.is_empty())
            .expect("a model provider")
    }

    fn template(value: Value) -> Template {
        serde_json::from_value(value).unwrap()
    }

    fn login_template() -> Value {
        json!({
            "title": "Admin login", "hint": "fill in", "itemType": "login", "name": "Backoffice",
            "fields": [
                {"key": "username", "label": "User", "type": "text", "value": "admin", "required": true},
                {"key": "password", "label": "Password", "type": "password", "required": true}
            ],
            "verify": {"type": "login", "url": "https://example.com/login", "okField": "ok"}
        })
    }

    fn api_key_template(provider_id: &str, base_url: &str) -> Value {
        json!({
            "title": "Key", "itemType": "api_key", "name": "My key", "providerId": provider_id,
            "fields": [
                {"key": "MY_API_KEY", "label": "Key", "type": "password", "required": true, "role": "api_key"},
                {"key": "MY_BASE_URL", "label": "URL", "type": "text", "value": base_url, "role": "base_url"},
                {"key": "MY_MODEL", "label": "Model", "type": "select", "role": "model"}
            ],
            "verify": {"type": "llm", "auth": "bearer"}
        })
    }

    fn group_template() -> Value {
        json!({
            "title": "Server", "itemType": "env_group", "name": "Server env", "category": "server",
            "fields": [
                {"key": "SSH_HOST", "label": "Host", "type": "text", "value": "10.0.0.1"},
                {"key": "SSH_PASSWORD", "label": "Password", "type": "password", "required": true},
                {"key": "SSH_TOKEN", "label": "Token", "type": "text", "role": "api_key"}
            ]
        })
    }

    fn reason(mut base: Value, change: impl FnOnce(&mut Value)) -> &'static str {
        change(&mut base);
        validate_template(&template(base), &registry()).unwrap_err()
    }

    fn vault(directory: &Path) -> VaultService {
        let mut vault =
            VaultService::open(Database::open(directory.join("vault.sqlite")).unwrap()).unwrap();
        vault.setup_master("a-long-test-master-password").unwrap();
        vault
    }

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// Loopback HTTP server answering every request with `status` and `body`; the raw requests
    /// (lower-cased) it received are collected for assertions.
    fn serve(status: u16, body: &str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let body = body.to_owned();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                while let Ok(read @ 1..) = stream.read(&mut buffer) {
                    request.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&request).to_lowercase();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&request).to_lowercase());
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{}", address.port()), seen)
    }

    fn login_verify(url: &str) -> Verify {
        template(json!({
            "title": "t", "itemType": "login", "name": "n",
            "fields": [{"key": "username", "label": "u", "type": "text"}],
            "verify": {"type": "login", "url": url, "okField": "ok"}
        }))
        .verify
        .unwrap()
    }

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    // -------------------------------------------------------------- validation

    #[test]
    fn valid_templates_pass() {
        let providers = registry();
        let id = provider().id;
        for value in [
            login_template(),
            api_key_template(&id, ""),
            group_template(),
            json!({"title": "t", "itemType": "login", "name": "n",
                   "fields": [{"key": "username", "label": "u", "type": "text"}],
                   "verify": {"type": "none"}}),
        ] {
            assert_eq!(validate_template(&template(value), &providers), Ok(()));
        }
    }

    #[test]
    fn every_reason_code_is_reported() {
        let id = provider().id;
        assert_eq!(
            reason(login_template(), |t| t["title"] = json!("")),
            "bad_shape"
        );
        assert_eq!(
            reason(login_template(), |t| t["itemType"] = json!("note")),
            "bad_shape"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"] = json!([])),
            "bad_shape"
        );
        assert_eq!(
            reason(login_template(), |t| t["verify"]["type"] = json!("magic")),
            "bad_shape"
        );
        assert_eq!(
            reason(login_template(), |t| t["verify"]["okField"] =
                json!("bad name!")),
            "bad_shape"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"][1]["key"] =
                json!("username")),
            "duplicate_key"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"][1]["value"] = json!("x")),
            "prefilled_secret"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"][0]["options"] =
                json!(["a"])),
            "options_not_select"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"][0]["key"] =
                json!("user_name")),
            "bad_key"
        );
        assert_eq!(
            reason(group_template(), |t| t["fields"][0]["key"] = json!("1BAD")),
            "bad_key"
        );
        assert_eq!(
            reason(login_template(), |t| {
                t["fields"][0]["key"] = json!("website");
                t["verify"] = json!({"type": "none"});
            }),
            "login_needs_username"
        );
        assert_eq!(
            reason(login_template(), |t| t["fields"][0]["role"] =
                json!("model")),
            "role_not_allowed"
        );
        assert_eq!(
            reason(group_template(), |t| t["fields"][0]["role"] =
                json!("api_key")),
            "duplicate_role"
        );
        assert_eq!(
            reason(api_key_template(&id, ""), |t| t["fields"][0]["type"] =
                json!("text")),
            "api_key_fields"
        );
        assert_eq!(
            reason(api_key_template(&id, ""), |t| {
                t["fields"][2].as_object_mut().unwrap().remove("role");
            }),
            "api_key_fields"
        );
        assert_eq!(
            reason(api_key_template(&id, ""), |t| t["providerId"] =
                json!("no-such-provider")),
            "unknown_provider"
        );
        assert_eq!(
            reason(api_key_template(&id, ""), |t| {
                t.as_object_mut().unwrap().remove("providerId");
            }),
            "unknown_provider"
        );
        assert_eq!(
            reason(group_template(), |t| t["verify"] =
                json!({"type": "login", "url": "https://x.com/l"})),
            "verify_mismatch"
        );
        assert_eq!(
            reason(login_template(), |t| t["verify"] =
                json!({"type": "llm", "auth": "bearer"})),
            "verify_mismatch"
        );
        for url in [
            "http://example.com/login",
            "https://user:pw@example.com/login",
            "ftp://example.com",
            "not a url",
        ] {
            assert_eq!(
                reason(login_template(), |t| t["verify"]["url"] = json!(url)),
                "bad_url",
                "{url}"
            );
        }
        for url in [
            "http://localhost:3000/l",
            "http://127.0.0.1/l",
            "http://[::1]:8/l",
        ] {
            let mut value = login_template();
            value["verify"]["url"] = json!(url);
            assert_eq!(
                validate_template(&template(value), &registry()),
                Ok(()),
                "{url}"
            );
        }
    }

    #[test]
    fn view_prefills_base_url_and_hides_nothing_secret() {
        let provider = provider();
        let request = template(api_key_template(&provider.id, ""));
        let view = build_view("input-1-a", "2026-11-16T08:00:00Z", &request, &registry());
        let base = view.fields.iter().find(|f| f.key == "MY_BASE_URL").unwrap();
        assert_eq!(base.value, provider.base_url);
        assert_eq!(view.provider_name.as_deref(), Some(provider.name.as_str()));
        assert_eq!(view.verify.kind, "llm");
        assert_eq!(view.verify.host, host_of(&provider.base_url));
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["itemType"], "api_key");
        assert_eq!(json["fields"][0]["type"], "password");
    }

    #[test]
    fn value_checks_require_known_keys_and_required_fields() {
        let request = template(login_template());
        assert_eq!(
            check_values(&request, &map(&[("username", "a"), ("evil", "x")])),
            Err(InputError::new("invalid_value"))
        );
        assert_eq!(
            check_values(&request, &map(&[("username", "a"), ("password", "")])),
            Err(InputError::with_field("missing_field", "password"))
        );
        let long = "x".repeat(MAX_VALUE_CHARS + 1);
        assert_eq!(
            check_values(&request, &map(&[("username", "a"), ("password", &long)])),
            Err(InputError::with_field("invalid_value", "password"))
        );
        let ok = check_values(
            &request,
            &map(&[("username", " a "), ("password", PASSWORD)]),
        )
        .unwrap();
        assert_eq!(ok["username"], "a");
        assert_eq!(ok["password"], PASSWORD);
        let llm = template(api_key_template(&provider().id, ""));
        assert_eq!(
            check_values(
                &llm,
                &map(&[
                    ("MY_API_KEY", API_KEY),
                    ("MY_BASE_URL", "http://example.com")
                ])
            ),
            Err(InputError::with_field("bad_url", "MY_BASE_URL"))
        );
    }

    // ------------------------------------------------------------ verification

    #[test]
    fn login_verification_outcomes() {
        let (url, seen) = serve(200, r#"{"ok":true}"#);
        let verify = login_verify(&format!("{url}/login"));
        assert_eq!(run(verify_login(&verify, "admin", PASSWORD)), Ok(()));
        let request = seen.lock().unwrap()[0].clone();
        assert!(request.starts_with("post /login"));
        assert!(request.contains(r#""username":"admin""#));

        let (url, _) = serve(401, &format!(r#"{{"error":"{PASSWORD}"}}"#));
        let error = run(verify_login(&login_verify(&url), "admin", PASSWORD)).unwrap_err();
        assert_eq!((error.code, error.http_status), ("unauthorized", Some(401)));
        let text = serde_json::to_string(&error).unwrap();
        assert!(!text.contains(PASSWORD) && !text.contains("admin"));

        let (url, _) = serve(500, "boom");
        let error = run(verify_login(&login_verify(&url), "a", "b")).unwrap_err();
        assert_eq!((error.code, error.http_status), ("http_error", Some(500)));

        let (url, _) = serve(200, r#"{"ok":false}"#);
        let error = run(verify_login(&login_verify(&url), "a", "b")).unwrap_err();
        assert_eq!(error.code, "not_ok");

        let (url, _) = serve(200, "<html>");
        let error = run(verify_login(&login_verify(&url), "a", "b")).unwrap_err();
        assert_eq!(error.code, "bad_response");

        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())
        };
        let error = run(verify_login(&login_verify(&closed), "a", "b")).unwrap_err();
        assert_eq!(error.code, "network");
    }

    #[test]
    fn model_fetch_sends_the_key_and_lists_models() {
        let (url, seen) = serve(200, r#"{"data":[{"id":"m2"},{"id":"m1"},{"id":"m1"}]}"#);
        let models = run(fetch_models(&format!("{url}/v1/"), API_KEY, "bearer")).unwrap();
        assert_eq!(models, vec!["m1", "m2"]);
        let request = seen.lock().unwrap()[0].clone();
        assert!(request.starts_with("get /v1/models"), "{request}");
        assert!(request.contains(&format!("authorization: bearer {API_KEY}")));

        let (url, seen) = serve(200, r#"{"data":[{"id":"claude-x"}],"has_more":false}"#);
        let models = run(fetch_models(&url, API_KEY, "x-api-key")).unwrap();
        assert_eq!(models, vec!["claude-x"]);
        assert!(seen.lock().unwrap()[0].contains(&format!("x-api-key: {API_KEY}")));

        let (url, _) = serve(401, &format!("bad key {API_KEY}"));
        let error = run(fetch_models(&url, API_KEY, "bearer")).unwrap_err();
        assert_eq!((error.code, error.http_status), ("unauthorized", Some(401)));
        assert!(!serde_json::to_string(&error).unwrap().contains(API_KEY));

        let (url, _) = serve(200, "not json");
        assert_eq!(
            run(fetch_models(&url, API_KEY, "bearer")).unwrap_err().code,
            "bad_response"
        );
        assert_eq!(
            run(fetch_models("http://example.com", API_KEY, "bearer"))
                .unwrap_err()
                .code,
            "bad_url"
        );
    }

    #[test]
    fn submission_verifies_through_the_template() {
        let (url, _) = serve(200, r#"{"data":[]}"#);
        let id = provider().id;
        let request = template(api_key_template(&id, &url));
        let values = check_values(
            &request,
            &map(&[("MY_API_KEY", API_KEY), ("MY_BASE_URL", &url)]),
        )
        .unwrap();
        assert_eq!(run(verify_submission(&request, &values)), Ok(()));
        let (bad, _) = serve(403, "{}");
        let values = check_values(
            &request,
            &map(&[("MY_API_KEY", API_KEY), ("MY_BASE_URL", &bad)]),
        )
        .unwrap();
        assert_eq!(
            run(verify_submission(&request, &values)).unwrap_err().code,
            "unauthorized"
        );
    }

    // ----------------------------------------------------------------- storing

    fn assert_undoable(vault: &VaultService, id: &str, batch: &str, counts: (i64, i64)) {
        let notices = vault.open_import_batches().unwrap();
        let notice = notices.iter().find(|n| n.batch_id == batch).expect("batch");
        assert_eq!((notice.created_groups, notice.other_items), counts);
        vault.undo_import_batch(batch).unwrap();
        assert!(vault
            .list_metadata()
            .unwrap()
            .iter()
            .all(|item| item.id != id));
    }

    #[test]
    fn stores_a_login() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let request = template(login_template());
        let values = check_values(
            &request,
            &map(&[("username", "admin"), ("password", PASSWORD)]),
        )
        .unwrap();
        let (id, batch) = store(&vault, &request, &values, &registry()).unwrap();
        let fields = vault.read_item_fields(&id).unwrap();
        assert_eq!(fields["username"], "admin");
        assert_eq!(fields["password"], PASSWORD);
        let (second, _) = store(&vault, &request, &values, &registry()).unwrap();
        let names: Vec<String> = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert!(
            names.contains(&"Backoffice".to_owned())
                && names.contains(&"Backoffice (2)".to_owned())
        );
        assert_ne!(id, second);
        assert_undoable(&vault, &id, &batch, (0, 1));
    }

    #[test]
    fn stores_an_api_key_with_endpoint_and_model() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let provider = provider();
        let request = template(api_key_template(&provider.id, ""));
        let values = check_values(
            &request,
            &map(&[
                ("MY_API_KEY", API_KEY),
                ("MY_BASE_URL", "https://llm.example.com/v1"),
                ("MY_MODEL", "m1"),
            ]),
        )
        .unwrap();
        let (id, batch) = store(&vault, &request, &values, &registry()).unwrap();
        assert_eq!(vault.read_secret(&id).unwrap(), API_KEY);
        let metadata = vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|m| m.id == id)
            .unwrap();
        assert_eq!(metadata.env_key, "MY_API_KEY");
        assert_eq!(metadata.provider_id, provider.id);
        assert_undoable(&vault, &id, &batch, (0, 1));
    }

    #[test]
    fn stores_an_env_group_with_secret_flags() {
        let dir = tempfile::tempdir().unwrap();
        let vault = vault(dir.path());
        let request = template(group_template());
        let values = check_values(
            &request,
            &map(&[
                ("SSH_HOST", "10.0.0.1"),
                ("SSH_PASSWORD", PASSWORD),
                ("SSH_TOKEN", API_KEY),
            ]),
        )
        .unwrap();
        let (id, batch) = store(&vault, &request, &values, &registry()).unwrap();
        let variables = vault.group_values(&id, None).unwrap();
        let get = |name: &str| {
            variables
                .iter()
                .find(|(n, _, _)| n == name)
                .unwrap()
                .clone()
        };
        assert_eq!(
            get("SSH_HOST"),
            ("SSH_HOST".into(), "10.0.0.1".into(), false)
        );
        assert_eq!(
            get("SSH_PASSWORD"),
            ("SSH_PASSWORD".into(), PASSWORD.into(), true)
        );
        assert!(get("SSH_TOKEN").2);
        assert_undoable(&vault, &id, &batch, (1, 0));
    }

    // ------------------------------------------------------------ queue files

    fn pending_json(id: &str, created: &str, template: &Value) -> String {
        json!({"version": 1, "requestId": id, "kind": "request_input",
               "createdAt": created, "template": template})
        .to_string()
    }

    #[test]
    fn cancel_writes_the_result_then_removes_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let (pending, results) = (dir.path().join("p"), dir.path().join("r"));
        fs::create_dir_all(&pending).unwrap();
        let id = "input-1763270000000-ab12cd";
        fs::write(pending.join(format!("{id}.json")), "{}").unwrap();
        finish_in(&pending, &results, id, Outcome::Cancelled).unwrap();
        assert!(!pending.join(format!("{id}.json")).exists());
        let result: Value =
            serde_json::from_str(&fs::read_to_string(results.join(format!("{id}.json"))).unwrap())
                .unwrap();
        assert_eq!(result["status"], "cancelled");
        assert_eq!(result["requestId"], id);
        assert!(finish_in(&pending, &results, "../evil", Outcome::Cancelled).is_err());
    }

    #[test]
    fn scan_serves_fresh_requests_and_fails_broken_or_expired_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (pending, results) = (dir.path().join("p"), dir.path().join("r"));
        fs::create_dir_all(&pending).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let write =
            |id: &str, text: String| fs::write(pending.join(format!("{id}.json")), text).unwrap();
        write(
            "input-2",
            pending_json("input-2", "2026-11-16T10:00:00Z", &login_template()),
        );
        write(
            "input-1",
            pending_json("input-1", "2026-11-16T09:00:00Z", &group_template()),
        );
        write(
            "input-old",
            pending_json("input-old", "2026-11-14T09:00:00Z", &login_template()),
        );
        let mut broken = login_template();
        broken["fields"][1]["value"] = json!(PASSWORD);
        write(
            "input-bad",
            pending_json("input-bad", "2026-11-16T09:00:00Z", &broken),
        );
        write("input-junk", "not json".to_owned());

        let views = scan_requests(&pending, &results, &registry(), now);
        let ids: Vec<&str> = views.iter().map(|view| view.request_id.as_str()).collect();
        assert_eq!(ids, ["input-1", "input-2"]);
        let read = |id: &str| -> Value {
            serde_json::from_str(&fs::read_to_string(results.join(format!("{id}.json"))).unwrap())
                .unwrap()
        };
        assert_eq!(read("input-old")["error"]["code"], "expired");
        for id in ["input-bad", "input-junk"] {
            assert_eq!(read(id)["status"], "failed");
            assert_eq!(read(id)["error"]["code"], "invalid_template");
            assert!(!pending.join(format!("{id}.json")).exists());
        }
        // A request is only readable under its own file name.
        assert!(read_pending(&pending, "input-1", &registry()).is_ok());
        assert_eq!(
            read_pending(&pending, "input-9", &registry())
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(
            read_pending(&pending, "plan-1", &registry())
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }

    #[test]
    fn results_and_errors_never_carry_submitted_values() {
        let dir = tempfile::tempdir().unwrap();
        let (pending, results) = (dir.path().join("p"), dir.path().join("r"));
        fs::create_dir_all(&pending).unwrap();
        let id = "input-1763270000000-ff";
        finish_in(
            &pending,
            &results,
            id,
            Outcome::Saved {
                item_id: "item-1",
                item_type: "login",
            },
        )
        .unwrap();
        let text = fs::read_to_string(results.join(format!("{id}.json"))).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["itemId", "itemType", "requestId", "status", "updatedAt"]
        );
        assert!(!text.contains(PASSWORD) && !text.contains(API_KEY));

        finish_in(&pending, &results, id, Outcome::Failed("expired")).unwrap();
        let text = fs::read_to_string(results.join(format!("{id}.json"))).unwrap();
        assert!(text.contains(r#""error":{"code":"expired"}"#));

        for error in [
            InputError::from_status(401),
            InputError::with_field("missing_field", "password"),
            InputError::new("network"),
        ] {
            let text = serde_json::to_string(&error).unwrap();
            assert!(!text.contains(PASSWORD) && !text.contains(API_KEY));
        }
        assert_eq!(
            serde_json::to_value(InputError::from_status(500)).unwrap(),
            json!({"code": "http_error", "httpStatus": 500})
        );
    }
}
