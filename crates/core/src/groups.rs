//! Config groups (`value_type = "env_group"`): named sets of environment-style variables.
//! Plain variables are stored as plaintext rows (so AI tools can read the harmless ones);
//! secret variables and the notes live only in the encrypted payload.

use crate::VaultError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

const TEMPLATES_JSON: &str = include_str!("../../../packages/item-types/group-templates.json");

pub const MAX_VARIABLES: usize = 200;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_NOTES_CHARS: usize = 100_000;
const PAYLOAD_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretRules {
    plain_contains: Vec<String>,
    plain_suffixes: Vec<String>,
    secret_contains: Vec<String>,
    secret_suffixes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TemplatesFile {
    #[serde(rename = "secretRules")]
    secret_rules: SecretRules,
    categories: Vec<String>,
}

fn templates() -> &'static TemplatesFile {
    static FILE: OnceLock<TemplatesFile> = OnceLock::new();
    FILE.get_or_init(|| {
        serde_json::from_str(TEMPLATES_JSON).expect("embedded group-templates.json is valid")
    })
}

/// Default for the secret toggle of a variable name; the user can always override it.
/// Plain markers win over secret markers (`PUBLIC_KEY`, `ACCESS_KEY_ID`).
pub fn is_secret_name(name: &str) -> bool {
    let rules = &templates().secret_rules;
    let upper = name.to_ascii_uppercase();
    if rules.plain_contains.iter().any(|part| upper.contains(part))
        || rules
            .plain_suffixes
            .iter()
            .any(|part| upper.ends_with(part))
    {
        return false;
    }
    rules
        .secret_contains
        .iter()
        .any(|part| upper.contains(part))
        || rules
            .secret_suffixes
            .iter()
            .any(|part| upper.ends_with(part))
}

pub fn valid_category(category: &str) -> bool {
    templates().categories.iter().any(|item| item == category)
}

pub fn valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && characters.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// One variable of a new group. A secret variable without a value is stored empty.
pub struct NewVariable {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

pub struct NewGroup {
    pub name: String,
    pub category: String,
    pub variables: Vec<NewVariable>,
    pub notes: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub ai_values: bool,
}

/// One variable of the full target list of an update.
pub struct VariableUpdate {
    pub name: String,
    pub secret: bool,
    /// `None` keeps the stored value (looked up under `from` when the variable is renamed).
    pub value: Option<String>,
    pub from: Option<String>,
}

pub struct GroupUpdate {
    pub name: String,
    pub category: String,
    pub variables: Vec<VariableUpdate>,
    /// `None` leaves the notes unchanged.
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub ai_values: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublicVariable {
    pub name: String,
    pub secret: bool,
    /// Present only for plain variables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroupPublic {
    pub variables: Vec<PublicVariable>,
    pub has_notes: bool,
    pub ai_values: bool,
}

/// Decrypted payload: secret values by variable name, plus the notes.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupPayload {
    pub v: u32,
    pub secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub notes: String,
}

impl GroupPayload {
    pub fn new(secrets: BTreeMap<String, String>, notes: String) -> Self {
        Self {
            v: PAYLOAD_VERSION,
            secrets,
            notes,
        }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("group payload serializes")
    }

    pub fn decode(plaintext: &str) -> Result<Self, VaultError> {
        let payload: Self = serde_json::from_str(plaintext)
            .map_err(|_| VaultError::InvalidField("payload".into()))?;
        if payload.v != PAYLOAD_VERSION {
            return Err(VaultError::InvalidField("payload".into()));
        }
        Ok(payload)
    }
}

pub fn check_value(name: &str, value: &str) -> Result<(), VaultError> {
    if value.len() > MAX_VALUE_BYTES || value.contains('\0') {
        return Err(VaultError::InvalidField(name.to_owned()));
    }
    Ok(())
}

pub fn check_notes(notes: &str) -> Result<(), VaultError> {
    if notes.chars().count() > MAX_NOTES_CHARS || notes.contains('\0') {
        return Err(VaultError::InvalidField("notes".into()));
    }
    Ok(())
}

/// Names must be valid identifiers, unique within the group, at most `MAX_VARIABLES`.
pub fn check_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<(), VaultError> {
    let mut seen = HashSet::new();
    for (index, name) in names.enumerate() {
        if index >= MAX_VARIABLES {
            return Err(VaultError::InvalidField("variables".into()));
        }
        if !valid_variable_name(name) || !seen.insert(name.to_owned()) {
            return Err(VaultError::InvalidField(name.to_owned()));
        }
    }
    Ok(())
}

pub fn subtitle_for(count: usize) -> String {
    format!("{count} 个变量")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_names_follow_the_documented_rules() {
        let secret = [
            "SSH_PASSWORD",
            "db_password",
            "DB_PASSWD",
            "ROOT_PWD",
            "OSS_ACCESS_KEY_SECRET",
            "BT_PANEL_API_TOKEN",
            "SSH_PRIVATE_KEY",
            "GCP_CREDENTIALS",
            "STRIPE_SK",
            "OPENAI_API_KEY",
            "AWS_ACCESS_KEY",
            "SIGNING_KEY",
        ];
        let plain = [
            "SSH_HOST",
            "SSH_PORT",
            "DB_NAME",
            "SSH_PUBLIC_KEY",
            "OSS_ACCESS_KEY_ID",
            "CLIENT_ID",
            "PUBLIC_API_KEY",
            "BT_PANEL_URL",
            "KEY",
            "MONKEY",
        ];
        for name in secret {
            assert!(is_secret_name(name), "{name} should be secret");
        }
        for name in plain {
            assert!(!is_secret_name(name), "{name} should be plain");
        }
    }

    #[test]
    fn names_and_values_are_validated() {
        assert!(valid_variable_name("SSH_HOST") && valid_variable_name("_x1"));
        assert!(
            !valid_variable_name("1X") && !valid_variable_name("A-B") && !valid_variable_name("")
        );
        assert!(check_names(["A", "B"].into_iter()).is_ok());
        assert!(check_names(["A", "A"].into_iter()).is_err());
        let many: Vec<String> = (0..201).map(|i| format!("V{i}")).collect();
        assert!(check_names(many.iter().map(String::as_str)).is_err());
        assert!(check_value("A", &"x".repeat(MAX_VALUE_BYTES)).is_ok());
        assert!(check_value("A", &"x".repeat(MAX_VALUE_BYTES + 1)).is_err());
        assert!(check_value("A", "bad\0").is_err());
        assert!(check_notes(&"x".repeat(MAX_NOTES_CHARS)).is_ok());
        assert!(check_notes(&"x".repeat(MAX_NOTES_CHARS + 1)).is_err());
        assert!(valid_category("server") && !valid_category("personal"));
    }

    #[test]
    fn payload_rejects_unknown_fields() {
        let payload = GroupPayload::new(BTreeMap::from([("A".into(), "b".into())]), "n".into());
        assert_eq!(GroupPayload::decode(&payload.encode()).unwrap(), payload);
        assert!(GroupPayload::decode(r#"{"v":1,"secrets":{},"notes":"","extra":1}"#).is_err());
        assert!(GroupPayload::decode(r#"{"v":2,"secrets":{}}"#).is_err());
        assert!(GroupPayload::decode("plain").is_err());
    }
}
