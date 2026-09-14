use secrethub_exporter::{
    detect_conflicts, ensure_gitignore_env, gitignore_has_env, parse_env_entries,
    parse_json_entries, render_env, render_example, EnvEntry,
};

#[test]
fn writes_values_with_spaces_hash_quotes_json_unicode_and_multiline_safely() {
    let entries = vec![
        EnvEntry::new("OPENAI_API_KEY", "fixture value #1"),
        EnvEntry::new("JSON_CONFIG", r#"{"mode":"local"}"#),
        EnvEntry::new("PRIVATE_KEY", "line one\nline two"),
        EnvEntry::new("UNICODE_LABEL", "你好 SecretHub"),
    ];
    let rendered = render_env(&entries).unwrap();

    assert!(rendered.contains("OPENAI_API_KEY=\"fixture value #1\""));
    assert!(rendered.contains("JSON_CONFIG=\"{\\\"mode\\\":\\\"local\\\"}\""));
    assert!(rendered.contains("PRIVATE_KEY=\"line one\\nline two\""));
    assert!(rendered.contains("UNICODE_LABEL=\"你好 SecretHub\""));
    assert!(!rendered.contains("OPENAI_API_KEY=fixture value #1"));
}

#[test]
fn renders_example_without_secret_values_and_reports_existing_key_conflicts() {
    let entries = vec![EnvEntry::new("OPENAI_API_KEY", "fixture value")];
    assert_eq!(render_example(&entries).unwrap(), "OPENAI_API_KEY=\"\"\n");

    let conflicts = detect_conflicts("OPENAI_API_KEY=old\nOTHER=value\n", &entries).unwrap();
    assert_eq!(conflicts, vec!["OPENAI_API_KEY"]);
}

#[test]
fn renders_notes_as_comments_without_exporting_secret_hub_metadata() {
    let entries = vec![EnvEntry::with_comment(
        "DEEPSEEK_API_KEY",
        "fixture-value",
        "local development only",
    )];
    let rendered = render_env(&entries).unwrap();
    assert_eq!(
        rendered,
        "# local development only\nDEEPSEEK_API_KEY=\"fixture-value\"\n"
    );
    assert!(!rendered.contains("value_type"));
    assert!(!rendered.contains("DEEPSEEK-V4.1-FLASH"));
}

#[test]
fn checks_and_explicitly_repairs_gitignore_without_touching_existing_rules() {
    let directory = tempfile::tempdir().unwrap();
    let gitignore = directory.path().join(".gitignore");
    assert!(!gitignore_has_env(&gitignore).unwrap());
    assert!(ensure_gitignore_env(&gitignore).unwrap());
    assert!(gitignore_has_env(&gitignore).unwrap());
    assert!(!ensure_gitignore_env(&gitignore).unwrap());
    assert_eq!(std::fs::read_to_string(gitignore).unwrap(), ".env\n");
}

#[test]
fn previews_env_and_json_imports_without_persisting_values() {
    let env_entries = parse_env_entries("OPENAI_API_KEY=fixture\nPORT=3000\n").unwrap();
    assert_eq!(
        env_entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        vec!["OPENAI_API_KEY", "PORT"]
    );
    let json_entries =
        parse_json_entries(r#"{"SUPABASE_URL":"https://example.invalid","DEBUG":true}"#).unwrap();
    assert_eq!(
        json_entries
            .iter()
            .find(|entry| entry.key == "SUPABASE_URL")
            .unwrap()
            .value,
        "https://example.invalid"
    );
    assert_eq!(
        json_entries
            .iter()
            .find(|entry| entry.key == "DEBUG")
            .unwrap()
            .value,
        "true"
    );
    assert!(parse_json_entries(r#"["not-an-object"]"#).is_err());
}
