//! Typed items: field definitions (single source: `packages/item-types/item-types.json`),
//! validation, the encrypted JSON payload format and the plaintext list subtitle.

use crate::VaultError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const ITEM_TYPES_JSON: &str = include_str!("../../../packages/item-types/item-types.json");

/// Marker stored in every JSON payload so old raw-string payloads can be told apart.
const PAYLOAD_VERSION: u32 = 1;

pub type Fields = BTreeMap<String, String>;

#[derive(Debug, Deserialize)]
pub struct FieldDef {
    pub key: String,
    pub kind: String,
    pub sensitive: bool,
    pub required: bool,
    #[serde(default, rename = "minLength")]
    pub min_length: usize,
    #[serde(rename = "maxLength")]
    pub max_length: usize,
    #[serde(default)]
    pub label: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemTypeDef {
    pub key: String,
    pub personal: bool,
    /// `json` for encrypted field documents, `string` for the raw-string developer types.
    pub storage: String,
    #[serde(default)]
    pub label: BTreeMap<String, String>,
    pub fields: Vec<FieldDef>,
}

impl ItemTypeDef {
    pub fn field(&self, key: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|field| field.key == key)
    }
}

#[derive(Debug, Deserialize)]
struct ItemTypesFile {
    types: Vec<ItemTypeDef>,
}

pub fn item_types() -> &'static [ItemTypeDef] {
    static TYPES: OnceLock<Vec<ItemTypeDef>> = OnceLock::new();
    TYPES.get_or_init(|| {
        serde_json::from_str::<ItemTypesFile>(ITEM_TYPES_JSON)
            .expect("embedded item-types.json is valid")
            .types
    })
}

/// A type whose payload is a JSON field document (login, bank_card, note).
pub fn json_item_type(key: &str) -> Option<&'static ItemTypeDef> {
    item_types()
        .iter()
        .find(|item| item.key == key && item.storage == "json")
}

fn invalid(field: &str) -> VaultError {
    VaultError::InvalidField(field.to_owned())
}

/// Luhn checksum over a digit string; used for a soft warning only.
pub fn luhn_valid(digits: &str) -> bool {
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let digit = u32::from(byte - b'0');
            if index % 2 == 1 {
                let doubled = digit * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                digit
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Host name of a stored website value; a bare `example.com` is accepted too.
pub fn host_of(website: &str) -> Option<String> {
    let trimmed = website.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = if trimmed.contains("://") {
        url::Url::parse(trimmed)
    } else {
        url::Url::parse(&format!("https://{trimmed}"))
    }
    .ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    parsed.host_str().map(str::to_owned)
}

fn has_control(value: &str, allow_newlines: bool) -> bool {
    value
        .chars()
        .any(|ch| ch.is_control() && !(allow_newlines && matches!(ch, '\n' | '\r' | '\t')))
}

/// Validates one value for its field kind and returns the normalized form to store.
/// The caller has already handled blank values.
pub fn normalize_field(def: &FieldDef, raw: &str) -> Result<String, VaultError> {
    let fail = || invalid(&def.key);
    let normalized = match def.kind.as_str() {
        "text" | "email_or_text" => {
            let value = raw.trim();
            if has_control(value, false) {
                return Err(fail());
            }
            value.to_owned()
        }
        "secret" => {
            if has_control(raw, false) {
                return Err(fail());
            }
            raw.to_owned()
        }
        "multiline" => {
            if has_control(raw, true) {
                return Err(fail());
            }
            raw.to_owned()
        }
        "url" => {
            let value = raw.trim();
            if has_control(value, false) || host_of(value).is_none() {
                return Err(fail());
            }
            value.to_owned()
        }
        "card_number" => {
            let digits: String = raw.chars().filter(|ch| !matches!(ch, ' ' | '-')).collect();
            if !digits.bytes().all(|byte| byte.is_ascii_digit())
                || !(12..=19).contains(&digits.len())
            {
                return Err(fail());
            }
            digits
        }
        "expiry" => {
            let value = raw.trim();
            let bytes = value.as_bytes();
            let valid = bytes.len() == 5
                && bytes[2] == b'/'
                && bytes[..2].iter().all(u8::is_ascii_digit)
                && bytes[3..].iter().all(u8::is_ascii_digit)
                && matches!(
                    &value[..2],
                    "01" | "02"
                        | "03"
                        | "04"
                        | "05"
                        | "06"
                        | "07"
                        | "08"
                        | "09"
                        | "10"
                        | "11"
                        | "12"
                );
            if !valid {
                return Err(fail());
            }
            value.to_owned()
        }
        "digits" => {
            let value = raw.trim();
            if !value.bytes().all(|byte| byte.is_ascii_digit())
                || value.len() < def.min_length
                || value.len() > def.max_length
            {
                return Err(fail());
            }
            value.to_owned()
        }
        "totp_secret" => {
            let compact: String = raw
                .chars()
                .filter(|ch| !ch.is_whitespace() && *ch != '-')
                .collect::<String>()
                .to_ascii_uppercase();
            let body = compact.trim_end_matches('=');
            if body.len() < 8 || !body.bytes().all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7')) {
                return Err(fail());
            }
            body.to_owned()
        }
        _ => return Err(fail()),
    };
    if normalized.chars().count() > def.max_length {
        return Err(fail());
    }
    Ok(normalized)
}

/// Normalizes the given fields; blank values are dropped. Unknown keys are rejected.
pub fn validate_fields(def: &ItemTypeDef, input: &Fields) -> Result<Fields, VaultError> {
    let mut out = Fields::new();
    for (key, value) in input {
        let field = def.field(key).ok_or_else(|| invalid(key))?;
        if value.trim().is_empty() {
            continue;
        }
        out.insert(key.clone(), normalize_field(field, value)?);
    }
    Ok(out)
}

/// Every required field must be present.
pub fn check_required(def: &ItemTypeDef, fields: &Fields) -> Result<(), VaultError> {
    for field in def.fields.iter().filter(|field| field.required) {
        if fields
            .get(&field.key)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(invalid(&field.key));
        }
    }
    Ok(())
}

/// `{"v":1,"fields":{...}}`
pub fn encode_payload(fields: &Fields) -> String {
    serde_json::json!({ "v": PAYLOAD_VERSION, "fields": fields }).to_string()
}

#[derive(Deserialize)]
struct StoredPayload {
    v: u32,
    fields: Fields,
}

/// Reads a payload. A plaintext that is not a versioned JSON document is interpreted the way
/// the legacy migration would, so a half-migrated vault stays readable.
pub fn decode_payload(value_type: &str, plaintext: &str) -> Fields {
    if let Ok(stored) = serde_json::from_str::<StoredPayload>(plaintext) {
        if stored.v == PAYLOAD_VERSION {
            return stored.fields;
        }
    }
    let key = match value_type {
        "login" => {
            let lower = plaintext.trim().to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                "website"
            } else {
                "password"
            }
        }
        "bank_card" => "card_number",
        "note" => "body",
        _ => return Fields::new(),
    };
    Fields::from([(key.to_owned(), plaintext.to_owned())])
}

/// Plaintext, low-sensitivity list line (typed-items design section 4).
pub fn subtitle_for(value_type: &str, fields: &Fields, env_key: &str) -> String {
    match value_type {
        "login" => fields
            .get("website")
            .and_then(|website| host_of(website))
            .unwrap_or_default(),
        "bank_card" => {
            let bank = fields.get("bank").map(String::as_str).unwrap_or_default();
            let number = fields
                .get("card_number")
                .map(String::as_str)
                .unwrap_or_default();
            let last4: String = number
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            match (bank.is_empty(), last4.is_empty()) {
                (_, true) => bank.to_owned(),
                (true, false) => format!("尾号 {last4}"),
                (false, false) => format!("{bank} · 尾号 {last4}"),
            }
        }
        "dev_secret" => env_key.to_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn embedded_definitions_are_consistent() {
        let known = [
            "text",
            "url",
            "email_or_text",
            "secret",
            "multiline",
            "card_number",
            "expiry",
            "digits",
            "totp_secret",
        ];
        for item in item_types() {
            for field in &item.fields {
                assert!(known.contains(&field.kind.as_str()), "{}", field.kind);
                assert!(field.max_length > 0);
                assert!(field.label.contains_key("zh-CN") && field.label.contains_key("en"));
            }
        }
        assert_eq!(
            item_types()
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            ["login", "bank_card", "note", "dev_secret"]
        );
        assert!(json_item_type("dev_secret").is_none());
        assert!(json_item_type("api_key").is_none());
    }

    #[test]
    fn login_fields_validate() {
        let def = json_item_type("login").unwrap();
        let ok = validate_fields(
            def,
            &fields(&[
                ("website", " https://accounts.example.com/login "),
                ("username", "fake@example.com"),
                ("password", "fake-password"),
                ("totp", "jbsw y3dp ehpk 3pxp"),
                ("notes", "line1\nline2"),
            ]),
        )
        .unwrap();
        assert_eq!(ok["website"], "https://accounts.example.com/login");
        assert_eq!(ok["totp"], "JBSWY3DPEHPK3PXP");
        check_required(def, &ok).unwrap();
        // Unknown key, bad URL, bad Base32 and a missing username are rejected.
        assert!(validate_fields(def, &fields(&[("pin", "1234")])).is_err());
        assert!(validate_fields(def, &fields(&[("website", "ftp://x.example")])).is_err());
        assert!(validate_fields(def, &fields(&[("totp", "not base32!")])).is_err());
        assert!(validate_fields(def, &fields(&[("totp", "ABC")])).is_err());
        assert!(check_required(def, &fields(&[("password", "fake-password")])).is_err());
        // A bare host is accepted for the website.
        assert!(validate_fields(def, &fields(&[("website", "example.com")])).is_ok());
        // Blank values are dropped instead of stored.
        let blank = validate_fields(def, &fields(&[("username", "u"), ("notes", "  ")])).unwrap();
        assert_eq!(blank.len(), 1);
    }

    #[test]
    fn bank_card_fields_validate() {
        let def = json_item_type("bank_card").unwrap();
        let ok = validate_fields(
            def,
            &fields(&[
                ("bank", "Fake Bank"),
                ("card_number", "4111 1111 1111 1111"),
                ("expiry", "09/30"),
                ("cvv", "123"),
                ("pin", "123456"),
            ]),
        )
        .unwrap();
        assert_eq!(ok["card_number"], "4111111111111111");
        check_required(def, &ok).unwrap();
        for (key, value) in [
            ("card_number", "4111 1111 111"), // 11 digits
            ("card_number", "4111 1111 1111 111x"),
            ("card_number", "41111111111111111111"), // 20 digits
            ("expiry", "13/30"),
            ("expiry", "9/30"),
            ("expiry", "09-30"),
            ("cvv", "12"),
            ("cvv", "12345"),
            ("cvv", "12a"),
            ("pin", "123"),
            ("pin", "1234567"),
        ] {
            assert!(
                validate_fields(def, &fields(&[(key, value)])).is_err(),
                "{key} {value}"
            );
        }
        assert!(check_required(def, &fields(&[("bank", "Fake Bank")])).is_err());
        // Luhn failure is not a validation error.
        assert!(validate_fields(def, &fields(&[("card_number", "4111111111111112")])).is_ok());
        assert!(luhn_valid("4111111111111111"));
        assert!(!luhn_valid("4111111111111112"));
        assert!(!luhn_valid(""));
    }

    #[test]
    fn note_body_is_required_and_bounded() {
        let def = json_item_type("note").unwrap();
        let ok = validate_fields(def, &fields(&[("body", "multi\nline fake note")])).unwrap();
        check_required(def, &ok).unwrap();
        assert!(check_required(
            def,
            &validate_fields(def, &fields(&[("body", " \n ")])).unwrap()
        )
        .is_err());
        let big = "x".repeat(100_001);
        assert!(validate_fields(def, &fields(&[("body", &big)])).is_err());
        assert!(validate_fields(def, &fields(&[("body", &"x".repeat(100_000))])).is_ok());
        assert!(validate_fields(def, &fields(&[("body", "bad\u{0}char")])).is_err());
    }

    #[test]
    fn subtitles_hold_only_low_sensitivity_text() {
        let login = fields(&[
            ("website", "https://accounts.example.com/signin"),
            ("username", "fake@example.com"),
            ("password", "fake-password"),
        ]);
        assert_eq!(subtitle_for("login", &login, ""), "accounts.example.com");
        assert_eq!(subtitle_for("login", &fields(&[("username", "u")]), ""), "");
        let card = fields(&[("bank", "Fake Bank"), ("card_number", "4111111111111111")]);
        assert_eq!(
            subtitle_for("bank_card", &card, ""),
            "Fake Bank · 尾号 1111"
        );
        assert_eq!(
            subtitle_for("note", &fields(&[("body", "secret text")]), ""),
            ""
        );
        assert_eq!(
            subtitle_for("dev_secret", &Fields::new(), "FAKE_TOKEN"),
            "FAKE_TOKEN"
        );
    }

    #[test]
    fn payload_round_trips_and_falls_back_for_raw_strings() {
        let original = fields(&[("username", "u"), ("password", "fake-password")]);
        let encoded = encode_payload(&original);
        assert!(encoded.starts_with("{\"fields\":") || encoded.contains("\"v\":1"));
        assert_eq!(decode_payload("login", &encoded), original);
        assert_eq!(
            decode_payload("login", "fake-password")["password"],
            "fake-password"
        );
        assert_eq!(
            decode_payload("login", "https://example.com")["website"],
            "https://example.com"
        );
        assert_eq!(decode_payload("note", "plain text")["body"], "plain text");
        assert_eq!(
            decode_payload("bank_card", "4111111111111111")["card_number"],
            "4111111111111111"
        );
        // JSON without the version marker is still treated as a raw legacy string.
        assert_eq!(
            decode_payload("note", "{\"fields\":{\"body\":\"x\"}}")["body"],
            "{\"fields\":{\"body\":\"x\"}}"
        );
    }
}
