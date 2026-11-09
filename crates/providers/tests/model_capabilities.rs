use secrethub_providers::{
    default_registry, default_rules, infer_capabilities, load_rules, parse_model_list,
    CapabilitySource, DiscoveredModel, Protocol, ProviderKind, CAPABILITY_VOCABULARY,
};
use serde_json::{json, Value};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
}

fn infer(provider: &str, model: &DiscoveredModel) -> (Vec<String>, CapabilitySource) {
    infer_capabilities(provider, model, &default_registry(), default_rules())
}

fn bare(id: &str) -> DiscoveredModel {
    DiscoveredModel {
        id: id.to_owned(),
        ..Default::default()
    }
}

fn caps(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn parses_openrouter_with_modalities_and_parameters() {
    let page = parse_model_list(Protocol::Openai, &fixture("openrouter.json")).unwrap();
    assert_eq!(page.models.len(), 6);
    assert_eq!(page.next_cursor, None);
    let by_id = |id: &str| page.models.iter().find(|m| m.id == id).unwrap();
    let banana = by_id("google/gemini-nano-banana-2.1");
    assert!(banana.output.contains(&"image".to_owned()));
    assert!(banana.hints.contains(&"reasoning".to_owned()));
    assert!(banana.display_name.is_some());
    let lyria = by_id("google/lyria-3-pro-preview");
    assert!(lyria.output.contains(&"audio".to_owned()));

    assert_eq!(
        infer("openrouter", banana),
        (
            caps(&["reasoning", "vision", "image_gen"]),
            CapabilitySource::Provider
        )
    );
    assert_eq!(
        infer("openrouter", lyria),
        (caps(&["vision", "tts"]), CapabilitySource::Provider)
    );
    assert_eq!(
        infer("openrouter", by_id("moondream/moondream3-preview")),
        (
            caps(&["chat", "reasoning", "vision"]),
            CapabilitySource::Provider
        )
    );
    assert_eq!(
        infer("openrouter", by_id("liquid/lfm-2.5-24b-a2b")),
        (caps(&["chat", "reasoning"]), CapabilitySource::Provider)
    );
    // provider text->text merged with the heuristic `coder` match
    assert_eq!(
        infer("openrouter", by_id("qwen/qwen3-coder-next")),
        (caps(&["chat", "code"]), CapabilitySource::Provider)
    );
}

#[test]
fn parses_plain_openai_style() {
    let page = parse_model_list(Protocol::Openai, &fixture("openai.json")).unwrap();
    let ids: Vec<_> = page.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-5.5", "text-embedding-3-large", "whisper-1"]);
    assert!(page.models[0].input.is_empty() && page.models[0].hints.is_empty());
    // no provider signal: catalog, then heuristic, then unknown
    assert_eq!(
        infer("openai", &page.models[0]),
        (vec![], CapabilitySource::Unknown)
    );
    assert_eq!(
        infer("openai", &page.models[1]),
        (caps(&["embedding"]), CapabilitySource::Heuristic)
    );
    assert_eq!(
        infer("openai", &page.models[2]),
        (caps(&["asr"]), CapabilitySource::Heuristic)
    );
}

#[test]
fn parses_anthropic_with_cursor() {
    let page = parse_model_list(Protocol::Anthropic, &fixture("anthropic.json")).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("claude-legacy-text"));
    assert_eq!(
        page.models[0].display_name.as_deref(),
        Some("Claude Sonnet 5")
    );
    assert_eq!(
        infer("anthropic", &page.models[0]),
        (
            caps(&["chat", "reasoning", "vision"]),
            CapabilitySource::Provider
        )
    );
    assert_eq!(
        infer("anthropic", &page.models[1]),
        (caps(&["chat"]), CapabilitySource::Provider)
    );
    let last = parse_model_list(
        Protocol::Anthropic,
        r#"{"data":[{"id":"a"}],"has_more":false,"last_id":"a"}"#,
    )
    .unwrap();
    assert_eq!(last.next_cursor, None);
}

#[test]
fn parses_gemini_with_page_token() {
    let page = parse_model_list(Protocol::Gemini, &fixture("gemini.json")).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("page-2-token"));
    assert_eq!(page.models[0].id, "gemini-3.5-flash");
    assert_eq!(
        page.models[0].display_name.as_deref(),
        Some("Gemini 3.5 Flash")
    );
    assert_eq!(page.models[1].id, "text-embedding-004");
    assert_eq!(
        infer("gemini", &page.models[0]),
        (caps(&["chat", "reasoning"]), CapabilitySource::Provider)
    );
    assert_eq!(
        infer("gemini", &page.models[1]),
        (caps(&["embedding"]), CapabilitySource::Provider)
    );
}

#[test]
fn parses_new_api_endpoint_types() {
    let page = parse_model_list(Protocol::Openai, &fixture("newapi.json")).unwrap();
    let results: Vec<_> = page.models.iter().map(|m| infer("88api", m)).collect();
    assert_eq!(results[0], (caps(&["chat"]), CapabilitySource::Provider));
    assert_eq!(
        results[1],
        (caps(&["embedding"]), CapabilitySource::Provider)
    );
    assert_eq!(results[2], (caps(&["rerank"]), CapabilitySource::Provider));
    assert_eq!(
        results[3],
        (caps(&["image_gen"]), CapabilitySource::Provider)
    );
}

#[test]
fn caps_list_and_truncates_untrusted_strings() {
    let items: Vec<Value> = (0..2500)
        .map(|i| json!({ "id": format!("m-{i}") }))
        .collect();
    let body = json!({ "data": items }).to_string();
    let page = parse_model_list(Protocol::Openai, &body).unwrap();
    assert_eq!(page.models.len(), 2000);

    let long = "x".repeat(5000);
    let body = json!({ "data": [{ "id": long, "name": long }, { "id": "" }, { "name": "no id" }] })
        .to_string();
    let page = parse_model_list(Protocol::Openai, &body).unwrap();
    assert_eq!(page.models.len(), 1);
    assert_eq!(page.models[0].id.chars().count(), 200);
    assert_eq!(
        page.models[0]
            .display_name
            .as_ref()
            .unwrap()
            .chars()
            .count(),
        200
    );
}

#[test]
fn rejects_malformed_bodies() {
    assert!(parse_model_list(Protocol::Openai, "not json").is_err());
    assert!(parse_model_list(Protocol::Openai, "{}").is_err());
    assert!(parse_model_list(Protocol::Gemini, r#"{"data":[]}"#).is_err());
}

#[test]
fn inference_priority_is_provider_catalog_heuristic_unknown() {
    // catalog beats heuristic: `deepseek-v4-flash-vision-exp` is annotated chat+vision
    let catalog = infer("deepseek", &bare("deepseek-v4-flash-vision-exp"));
    assert_eq!(
        catalog,
        (caps(&["chat", "vision"]), CapabilitySource::Catalog)
    );
    // same ID on a provider that has no annotation falls back to heuristic
    let heuristic = infer("generic", &bare("deepseek-v4-flash-vision-exp"));
    assert_eq!(heuristic, (caps(&["vision"]), CapabilitySource::Heuristic));
    // provider beats catalog
    let model = DiscoveredModel {
        id: "deepseek-v4-flash-vision-exp".to_owned(),
        output: caps(&["text"]),
        ..Default::default()
    };
    assert_eq!(
        infer("deepseek", &model),
        (caps(&["chat", "vision"]), CapabilitySource::Provider)
    );
    assert_eq!(
        infer("deepseek", &bare("deepseek-v4-pro")),
        (vec![], CapabilitySource::Unknown)
    );
}

#[test]
fn rules_do_not_match_ordinary_chat_models() {
    let media = [
        "embedding",
        "rerank",
        "tts",
        "asr",
        "image_gen",
        "video_gen",
    ];
    for provider in default_registry() {
        for model in &provider.models {
            let (found, _) = infer(&provider.id, &bare(&model.id));
            if model.id == "qwen-audio-3.0-tts-plus" {
                assert_eq!(found, caps(&["tts"]));
                continue;
            }
            assert!(
                found.iter().all(|cap| !media.contains(&cap.as_str())),
                "{}/{} -> {found:?}",
                provider.id,
                model.id
            );
        }
    }
    for id in [
        "gpt-5.5",
        "claude-opus-5",
        "grok-4.5",
        "llama-4-maverick",
        "deepseek-v4-pro",
        "gemini-3.5-flash",
        "gpt-4o",
        "mistral-large-latest",
        "qwen3-235b-a22b",
        "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        "nvidia/llama-3.3-nemotron-super-49b-v1",
        "sdk-assistant-pro",
        "e50-chat",
        "gpt-4-turbo",
        "mimo-v2.5-pro",
    ] {
        assert_eq!(infer("generic", &bare(id)).0, Vec::<String>::new(), "{id}");
    }
}

#[test]
fn rules_match_intended_names() {
    for (id, cap) in [
        ("deepseek-r1-distill", "reasoning"),
        ("deepseek/deepseek-r1", "reasoning"),
        ("o3-mini", "reasoning"),
        ("multilingual-e5-large", "embedding"),
        ("BAAI/bge-m3", "embedding"),
        ("gpt-4o-mini-tts", "tts"),
        ("black-forest-labs/flux.1-schnell", "image_gen"),
        ("veo-3.0-generate", "video_gen"),
        ("qwen2.5-vl-72b", "vision"),
        ("Qwen3-Coder-480B", "code"),
        ("bge-reranker-v2", "rerank"),
    ] {
        let found = infer("generic", &bare(id)).0;
        assert!(found.iter().any(|c| c == cap), "{id} -> {found:?}");
    }
}

#[test]
fn catalog_and_rules_only_use_the_vocabulary() {
    for provider in default_registry() {
        for model in provider.models {
            for cap in model.capabilities {
                assert!(CAPABILITY_VOCABULARY.contains(&cap.as_str()), "{cap}");
            }
        }
    }
    let rules = load_rules(include_str!(
        "../../../packages/provider-definitions/capability-rules.json"
    ))
    .unwrap();
    assert!(!rules.is_empty());
    for rule in &rules {
        assert!(!rule.capabilities.is_empty());
        for cap in &rule.capabilities {
            assert!(CAPABILITY_VOCABULARY.contains(&cap.as_str()), "{cap}");
        }
    }
    assert!(load_rules(r#"[{"pattern":"(","capabilities":["chat"]}]"#).is_err());
    assert!(load_rules(r#"[{"pattern":"x","capabilities":["magic"]}]"#).is_err());
}

#[test]
fn catalog_marks_aggregators_and_protocols() {
    let registry = default_registry();
    let get = |id: &str| registry.iter().find(|p| p.id == id).unwrap();
    assert_eq!(get("openrouter").kind, ProviderKind::Aggregator);
    assert_eq!(get("openai").kind, ProviderKind::Direct);
    assert_eq!(get("anthropic").protocol, Protocol::Anthropic);
    assert_eq!(get("gemini").protocol, Protocol::Gemini);
    assert_eq!(get("deepseek").protocol, Protocol::Openai);
}
