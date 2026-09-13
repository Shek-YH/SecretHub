use secrethub_providers::{default_registry, detect_provider};

#[test]
fn detects_high_frequency_providers_locally_without_network_probing() {
    let registry = default_registry();
    assert_eq!(registry.len(), 10);
    assert_eq!(detect_provider("sk-proj-fixture", &registry).unwrap().id, "openai");
    assert_eq!(detect_provider("sk-ant-fixture", &registry).unwrap().id, "anthropic");
    assert_eq!(detect_provider("AIzaFixture", &registry).unwrap().id, "gemini");
    assert_eq!(detect_provider("unknown-value", &registry), None);
}
