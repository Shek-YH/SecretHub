use secrethub_providers::{default_registry, detect_provider, validate_endpoint};

#[test]
fn detects_high_frequency_providers_locally_without_network_probing() {
    let registry = default_registry();
    assert_eq!(registry.len(), 10);
    assert_eq!(detect_provider("sk-proj-fixture", &registry).unwrap().id, "openai");
    assert_eq!(detect_provider("sk-ant-fixture", &registry).unwrap().id, "anthropic");
    assert_eq!(detect_provider("AIzaFixture", &registry).unwrap().id, "gemini");
    assert_eq!(detect_provider("unknown-value", &registry), None);
}

#[test]
fn validators_allow_only_fixed_https_origins_and_reject_ssrf_targets() {
    assert!(validate_endpoint("openai", "https://api.openai.com/v1/models", &default_registry()).is_ok());
    assert!(validate_endpoint("openai", "http://api.openai.com/v1/models", &default_registry()).is_err());
    assert!(validate_endpoint("openai", "https://127.0.0.1/v1/models", &default_registry()).is_err());
    assert!(validate_endpoint("openai", "https://attacker.example/v1/models", &default_registry()).is_err());
}
