use secrethub_providers::{
    default_registry, detect_provider, validate_endpoint, validation_endpoint,
};

#[test]
fn detects_high_frequency_providers_locally_without_network_probing() {
    let registry = default_registry();
    for id in [
        "openai",
        "anthropic",
        "gemini",
        "deepseek",
        "xai",
        "openrouter",
        "github",
        "supabase",
        "cloudflare",
        "generic",
    ] {
        assert!(registry.iter().any(|provider| provider.id == id), "{id}");
    }
    assert_eq!(
        detect_provider("sk-proj-fixture", &registry).unwrap().id,
        "openai"
    );
    assert_eq!(
        detect_provider("sk-ant-fixture", &registry).unwrap().id,
        "anthropic"
    );
    assert_eq!(
        detect_provider("AIzaFixture", &registry).unwrap().id,
        "gemini"
    );
    assert_eq!(detect_provider("unknown-value", &registry), None);
}

#[test]
fn validators_allow_only_fixed_https_origins_and_reject_ssrf_targets() {
    assert!(validate_endpoint(
        "openai",
        "https://api.openai.com/v1/models",
        &default_registry()
    )
    .is_ok());
    assert!(validate_endpoint(
        "openai",
        "http://api.openai.com/v1/models",
        &default_registry()
    )
    .is_err());
    assert!(
        validate_endpoint("openai", "https://127.0.0.1/v1/models", &default_registry()).is_err()
    );
    assert!(validate_endpoint(
        "openai",
        "https://attacker.example/v1/models",
        &default_registry()
    )
    .is_err());
}

#[test]
fn exposes_fixed_low_cost_validation_endpoints_without_a_custom_url() {
    assert_eq!(
        validation_endpoint("openai"),
        Some("https://api.openai.com/v1/models")
    );
    assert_eq!(
        validation_endpoint("deepseek"),
        Some("https://api.deepseek.com/models")
    );
    assert_eq!(validation_endpoint("custom"), None);
}

#[test]
fn keeps_previous_fixed_origins_and_derives_new_ones_from_the_shared_catalog() {
    let registry = default_registry();
    for (provider, endpoint) in [
        ("openai", "https://api.openai.com/v1/models"),
        ("anthropic", "https://api.anthropic.com/v1/models"),
        (
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/models",
        ),
        ("deepseek", "https://api.deepseek.com/models"),
        ("xai", "https://api.x.ai/v1/models"),
        ("openrouter", "https://openrouter.ai/api/v1/models"),
        ("github", "https://api.github.com/user"),
        ("supabase", "https://api.supabase.com/v1/projects"),
        ("cloudflare", "https://api.cloudflare.com/client/v4/user"),
        ("kimi", "https://api.moonshot.cn/v1/models"),
    ] {
        assert!(
            validate_endpoint(provider, endpoint, &registry).is_ok(),
            "{provider}"
        );
    }
    assert!(validate_endpoint("generic", "https://api.openai.com/v1", &registry).is_err());
    assert!(validate_endpoint("custom", "https://api.openai.com/v1", &registry).is_err());
}
