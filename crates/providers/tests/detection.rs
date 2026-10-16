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

#[test]
fn models_endpoint_prefers_validation_url_and_falls_back_to_base_url_models() {
    use secrethub_providers::{models_endpoint, Protocol};
    let openai = models_endpoint("openai").unwrap();
    assert_eq!(openai.url, "https://api.openai.com/v1/models");
    assert!(!openai.is_fallback && openai.lists_models);
    assert_eq!(
        models_endpoint("anthropic").unwrap().protocol,
        Protocol::Anthropic
    );
    // Non-AI services get no guessed `/models` URL, so their tokens are never sent there.
    assert_eq!(models_endpoint("github"), None);
    assert!(models_endpoint("xiaomi").unwrap().is_fallback);
    assert_eq!(models_endpoint("custom"), None);
    assert_eq!(models_endpoint("no-such-provider"), None);
    let registry = default_registry();
    let mut fallbacks = 0;
    for provider in &registry {
        if let Some(endpoint) = models_endpoint(&provider.id) {
            assert!(endpoint.url.starts_with("https://"), "{}", provider.id);
            fallbacks += usize::from(endpoint.is_fallback);
        } else {
            assert!(provider.validation_url.is_none(), "{} dropped", provider.id);
        }
    }
    assert!(fallbacks > 0, "expected at least one fallback provider");
}

#[test]
fn maps_http_status_to_validation_status() {
    use secrethub_providers::map_validation_status as map;
    assert_eq!(map(200, false).status, "valid");
    assert_eq!(map(401, true).status, "unauthorized");
    assert_eq!(map(403, false).status, "unauthorized");
    assert_eq!(map(429, true).status, "rate_limited");
    assert_eq!(map(404, false).status, "invalid");
    assert_eq!(map(405, false).status, "invalid");
    let missing = map(404, true);
    assert_eq!(
        (missing.status, missing.models_unsupported),
        ("unsupported", true)
    );
    assert_eq!(map(405, true).status, "unsupported");
    assert_eq!(map(400, true).status, "invalid");
    assert_eq!(map(500, false).status, "network_error");
    assert_eq!(map(302, false).status, "network_error");
}

#[test]
fn builds_pagination_urls_per_protocol() {
    use secrethub_providers::{model_page_url, Protocol};
    assert_eq!(
        model_page_url(
            "https://api.anthropic.com/v1/models",
            Protocol::Anthropic,
            None
        )
        .unwrap(),
        "https://api.anthropic.com/v1/models?limit=1000"
    );
    assert_eq!(
        model_page_url(
            "https://api.anthropic.com/v1/models",
            Protocol::Anthropic,
            Some("m1")
        )
        .unwrap(),
        "https://api.anthropic.com/v1/models?limit=1000&after_id=m1"
    );
    assert_eq!(
        model_page_url(
            "https://g.example/v1beta/models",
            Protocol::Gemini,
            Some("a b")
        )
        .unwrap(),
        "https://g.example/v1beta/models?pageSize=1000&pageToken=a+b"
    );
    assert_eq!(
        model_page_url("https://x/models", Protocol::Openai, Some("c")).unwrap(),
        "https://x/models"
    );
}
