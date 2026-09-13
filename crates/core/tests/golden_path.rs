use secrethub_core::{NewSecretInput, VaultService};
use secrethub_exporter::{
    ensure_gitignore_env, render_env, render_example, validate_export_directory, write_atomic,
    EnvEntry,
};
use secrethub_storage::Database;

#[test]
fn creates_reopens_unlocks_selects_and_exports_without_plaintext_in_database() {
    let workspace = tempfile::tempdir().unwrap();
    let database_path = workspace.path().join("vault.sqlite");
    let project_path = workspace.path().join("project");
    std::fs::create_dir(&project_path).unwrap();
    let mut vault = VaultService::open(Database::open(&database_path).unwrap()).unwrap();
    vault.setup_master("correct horse battery staple").unwrap();
    vault
        .create_secret(NewSecretInput {
            name: "OpenAI Main".into(),
            provider_id: "openai".into(),
            env_key: "OPENAI_API_KEY".into(),
            description: "synthetic integration fixture".into(),
            tags: vec!["ai".into()],
            value: "fixture-golden-value".into(),
        })
        .unwrap();
    drop(vault);

    let mut reopened = VaultService::open(Database::open(&database_path).unwrap()).unwrap();
    reopened.unlock("correct horse battery staple").unwrap();
    let metadata = reopened.list_metadata().unwrap();
    assert_eq!(metadata.len(), 1);
    let entries = vec![EnvEntry::new(
        &metadata[0].env_key,
        reopened.read_secret(&metadata[0].id).unwrap(),
    )];
    let target = validate_export_directory(&project_path).unwrap();
    ensure_gitignore_env(&target.join(".gitignore")).unwrap();
    write_atomic(&target.join(".env"), &render_env(&entries).unwrap(), false).unwrap();
    write_atomic(
        &target.join(".env.example"),
        &render_example(&entries).unwrap(),
        false,
    )
    .unwrap();
    assert!(std::fs::read_to_string(target.join(".env"))
        .unwrap()
        .contains("OPENAI_API_KEY=\"fixture-golden-value\""));
    assert_eq!(
        std::fs::read_to_string(target.join(".env.example")).unwrap(),
        "OPENAI_API_KEY=\"\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(target.join(".gitignore")).unwrap(),
        ".env\n"
    );
    let sqlite_text = std::fs::read(&database_path).unwrap();
    assert!(!sqlite_text
        .windows(b"fixture-golden-value".len())
        .any(|window| window == b"fixture-golden-value"));
}
