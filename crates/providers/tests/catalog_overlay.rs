use secrethub_providers::{
    bundled_serial, default_registry, default_rules, effective_catalog, effective_models,
    infer_capabilities, parse_overlay, CapabilitySource, CatalogOrigin, DiscoveredModel,
    MAX_CATALOG_BYTES,
};

const BUNDLED_OVERLAY: &str =
    include_str!("../../../packages/provider-definitions/model-catalog.json");

fn overlay_json(providers: &str) -> String {
    format!(
        r#"{{"schema":1,"serial":2026101699,"publishedAt":"2026-10-17T00:00:00Z","providers":{providers}}}"#
    )
}

#[test]
fn shipped_model_catalog_is_valid_and_not_older_than_the_bundled_serial() {
    // Between editing model-catalog.json and `catalog:bake` the two may differ in content.
    let overlay = parse_overlay(BUNDLED_OVERLAY.as_bytes()).expect("shipped catalog parses");
    assert!(overlay.serial >= bundled_serial().serial);
    let merged = effective_catalog(&default_registry(), Some(&overlay));
    assert_eq!(merged.providers.len(), default_registry().len());
}

#[test]
fn rejects_unknown_fields_anywhere() {
    let top = r#"{"schema":1,"serial":5,"publishedAt":"2026-10-17T00:00:00Z","providers":{},"baseUrl":"https://evil.example"}"#;
    assert!(parse_overlay(top.as_bytes()).is_err());
    for body in [
        r#"{"deepseek":{"models":[{"id":"x"}],"baseUrl":"https://evil.example"}}"#,
        r#"{"deepseek":{"models":[{"id":"x","url":"https://evil.example"}]}}"#,
        r#"{"deepseek":{"models":[{"id":"x","keyUrl":"https://evil.example"}]}}"#,
    ] {
        assert!(
            parse_overlay(overlay_json(body).as_bytes()).is_err(),
            "{body}"
        );
    }
}

#[test]
fn rejects_invalid_values() {
    let bad = [
        r#"{"deepseek":{"models":[{"id":""}]}}"#,
        r#"{"deepseek":{"models":[{"id":"a\u0007b"}]}}"#,
        r#"{"deepseek":{"models":[{"id":"x","capabilities":["telepathy"]}]}}"#,
        r#"{"deepseek":{"models":[{"id":"x"},{"id":"x"}]}}"#,
        r#"{"deepseek":{"models":[]}}"#,
        r#"{"deepseek":{"defaultModel":"missing","models":[{"id":"x"}]}}"#,
        r#"{"deepseek":{"defaultModel":"x","models":[{"id":"x","deprecated":true}]}}"#,
    ];
    for body in bad {
        assert!(
            parse_overlay(overlay_json(body).as_bytes()).is_err(),
            "{body}"
        );
    }
    let long_id = "a".repeat(201);
    let body = format!(r#"{{"deepseek":{{"models":[{{"id":"{long_id}"}}]}}}}"#);
    assert!(parse_overlay(overlay_json(&body).as_bytes()).is_err());

    let models = |count: usize| {
        let items: Vec<String> = (0..count).map(|i| format!(r#"{{"id":"m{i}"}}"#)).collect();
        overlay_json(&format!(
            r#"{{"deepseek":{{"models":[{}]}}}}"#,
            items.join(",")
        ))
    };
    assert!(parse_overlay(models(301).as_bytes()).is_err());
    assert!(parse_overlay(models(300).as_bytes()).is_ok());

    assert!(parse_overlay(&vec![b' '; MAX_CATALOG_BYTES + 1]).is_err());
    let schema2 = r#"{"schema":2,"serial":1,"publishedAt":"2026-10-17T00:00:00Z","providers":{}}"#;
    assert!(parse_overlay(schema2.as_bytes()).is_err());
    assert!(parse_overlay(b"not json").is_err());
}

#[test]
fn ignores_unknown_providers_and_replaces_lists_wholesale() {
    let json = overlay_json(
        r#"{
        "deepseek":{"defaultModel":"ds-new","models":[
            {"id":"ds-new","label":"DS New","capabilities":["chat","reasoning","chat"]},
            {"id":"deepseek-v4-flash","deprecated":true,"capabilities":["chat"]}]},
        "not-a-provider":{"models":[{"id":"x"}]}}"#,
    );
    let overlay = parse_overlay(json.as_bytes()).unwrap();
    let bundled = default_registry();
    let merged = effective_catalog(&bundled, Some(&overlay));
    assert_eq!(merged.origin, CatalogOrigin::Remote);
    assert_eq!(merged.serial, 2026101699);
    assert_eq!(merged.providers.len(), bundled.len());
    assert!(merged.providers.iter().all(|p| p.id != "not-a-provider"));
    let before = bundled.iter().find(|p| p.id == "deepseek").unwrap();
    let after = merged
        .providers
        .iter()
        .find(|p| p.id == "deepseek")
        .unwrap();
    assert_eq!(
        after
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["ds-new", "deepseek-v4-flash"]
    );
    assert_eq!(after.models[0].capabilities, ["chat", "reasoning"]);
    assert_eq!(after.default_model.as_deref(), Some("ds-new"));
    assert!(after.models[1].deprecated);
    // URLs, env names and protocol never change.
    assert_eq!(after.base_url, before.base_url);
    assert_eq!(after.env_keys, before.env_keys);
    assert_eq!(after.validation_url, before.validation_url);
    assert_eq!(after.protocol, before.protocol);
    // Other providers are untouched.
    for provider in bundled.iter().filter(|p| p.id != "deepseek") {
        assert_eq!(
            merged.providers.iter().find(|p| p.id == provider.id),
            Some(provider)
        );
    }
}

#[test]
fn deprecated_models_still_feed_capability_inference() {
    let json = overlay_json(
        r#"{"deepseek":{"models":[
        {"id":"ds-new"},{"id":"ds-old-vision","deprecated":true,"capabilities":["chat","vision"]}]}}"#,
    );
    let overlay = parse_overlay(json.as_bytes()).unwrap();
    let merged = effective_catalog(&default_registry(), Some(&overlay));
    let model = DiscoveredModel {
        id: "ds-old-vision".into(),
        ..Default::default()
    };
    let (caps, source) = infer_capabilities("deepseek", &model, &merged.providers, default_rules());
    assert_eq!(caps, ["chat", "vision"]);
    assert_eq!(source, CapabilitySource::Catalog);
    // Models removed by the overlay no longer match the catalog source.
    let removed = DiscoveredModel {
        id: "deepseek-v4-flash-vision-exp".into(),
        ..Default::default()
    };
    let (_, source) = infer_capabilities("deepseek", &removed, &merged.providers, default_rules());
    assert_eq!(source, CapabilitySource::Heuristic);
}

#[test]
fn effective_models_view_has_no_urls() {
    let view = effective_models(&effective_catalog(&default_registry(), None));
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains("http"));
    assert!(!json.contains("baseUrl"));
    assert!(json.contains("\"source\":\"bundled\""));
    assert_eq!(view.serial, bundled_serial().serial);
}
