use serde_json::Value;

const SENSITIVE_FIELDS: &[&str] = &[
    "api_key",
    "apikey",
    "token",
    "password",
    "secret",
    "authorization",
    "cookie",
    "private_key",
];

fn is_sensitive(name: &str) -> bool {
    let normalized = name
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .to_ascii_lowercase();
    SENSITIVE_FIELDS.iter().any(|field| *field == normalized)
}

pub fn redact_text(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            let separator = token.find('=').or_else(|| token.find(':'));
            let Some(index) = separator else {
                return token.to_owned();
            };
            let (raw_key, raw_value) = token.split_at(index);
            let separator = &raw_value[..1];
            if is_sensitive(raw_key) {
                format!("{}{}[REDACTED]", raw_key, separator)
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn redact_json(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive(key) {
                    *child = Value::String("[REDACTED]".to_owned());
                } else {
                    let current = std::mem::take(child);
                    *child = redact_json(current);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                let current = std::mem::take(item);
                *item = redact_json(current);
            }
        }
        _ => {}
    }
    value
}
