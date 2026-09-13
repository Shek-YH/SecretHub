use secrethub_redaction::{redact_json, redact_text};
use serde_json::json;

#[test]
fn redacts_sensitive_key_value_pairs_without_touching_safe_metadata() {
    let result =
        redact_text("provider=OpenAI api_key=fixture-value status=valid token:fixture-token");

    assert!(result.contains("provider=OpenAI"));
    assert!(result.contains("api_key=[REDACTED]"));
    assert!(result.contains("token:[REDACTED]"));
    assert!(!result.contains("fixture-value"));
    assert!(!result.contains("fixture-token"));
}

#[test]
fn redacts_sensitive_json_fields_recursively() {
    let result = redact_json(
        json!({"provider":"OpenAI","api_key":"fixture-value","nested":{"cookie":"fixture-cookie"}}),
    );

    assert_eq!(result["provider"], "OpenAI");
    assert_eq!(result["api_key"], "[REDACTED]");
    assert_eq!(result["nested"]["cookie"], "[REDACTED]");
}
