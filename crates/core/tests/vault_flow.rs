use secrethub_core::{NewSecretInput, VaultService};
use secrethub_storage::Database;

fn temp_database() -> (std::path::PathBuf, Database) {
    let path =
        std::env::temp_dir().join(format!("secrethub-vault-{}.sqlite", uuid::Uuid::new_v4()));
    let _ = std::fs::remove_file(&path);
    (path.clone(), Database::open(&path).unwrap())
}

#[test]
fn creates_secret_then_reopens_and_unlocks_without_returning_plaintext_metadata() {
    let (path, database) = temp_database();
    let mut vault = VaultService::open(database).unwrap();
    vault.setup_master("correct horse battery staple").unwrap();
    vault
        .create_secret(NewSecretInput {
            name: "OpenAI Main".into(),
            provider_id: "openai".into(),
            env_key: "OPENAI_API_KEY".into(),
            description: "local development".into(),
            tags: vec!["ai".into()],
            value: "fixture-secret-value".into(),
        })
        .unwrap();
    assert_eq!(vault.list_metadata().unwrap().len(), 1);
    assert_eq!(vault.list_metadata().unwrap()[0].env_key, "OPENAI_API_KEY");
    vault.lock();
    assert!(vault.list_metadata().is_err());
    drop(vault);

    let reopened = Database::open(&path).unwrap();
    let mut unlocked = VaultService::open(reopened).unwrap();
    assert!(unlocked.unlock("wrong password").is_err());
    unlocked.unlock("correct horse battery staple").unwrap();
    let metadata = unlocked.list_metadata().unwrap();
    assert_eq!(metadata[0].name, "OpenAI Main");
    let value = unlocked.read_secret(metadata[0].id.as_str()).unwrap();
    assert_eq!(value, "fixture-secret-value");
    let _ = std::fs::remove_file(path);
}

#[test]
fn updates_and_deletes_secret_through_the_vault_service() {
    let (_path, database) = temp_database();
    let mut vault = VaultService::open(database).unwrap();
    vault.setup_master("correct horse battery staple").unwrap();
    let id = vault
        .create_secret(NewSecretInput {
            name: "Before".into(),
            provider_id: "generic".into(),
            env_key: "BEFORE".into(),
            description: String::new(),
            tags: vec![],
            value: "fixture-before".into(),
        })
        .unwrap();
    vault
        .update_secret(
            &id,
            NewSecretInput {
                name: "After".into(),
                provider_id: "generic".into(),
                env_key: "AFTER".into(),
                description: "updated".into(),
                tags: vec!["changed".into()],
                value: "fixture-after".into(),
            },
        )
        .unwrap();
    assert_eq!(vault.list_metadata().unwrap()[0].env_key, "AFTER");
    assert_eq!(vault.read_secret(&id).unwrap(), "fixture-after");
    assert!(vault.delete_secret(&id).unwrap());
    assert!(vault.list_metadata().unwrap().is_empty());
}
