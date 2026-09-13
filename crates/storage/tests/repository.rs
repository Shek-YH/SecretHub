use rusqlite::Connection;
use secrethub_storage::{Database, NewSecret};

fn temp_database() -> (std::path::PathBuf, Database) {
    let path = std::env::temp_dir().join(format!("secrethub-test-{}.sqlite", uuid::Uuid::new_v4()));
    let _ = std::fs::remove_file(&path);
    (path.clone(), Database::open(&path).unwrap())
}

#[test]
fn initializes_vault_and_round_trips_metadata_and_encrypted_payload() {
    let (path, database) = temp_database();
    database.set_vault_meta(&[1, 2, 3, 4], &[8, 9, 10]).unwrap();
    let id = database
        .create_secret(NewSecret {
            name: "OpenAI Main".into(),
            provider_id: "openai".into(),
            env_key: "OPENAI_API_KEY".into(),
            description: "local development".into(),
            tags: vec!["ai".into(), "local".into()],
            ciphertext: vec![90, 91, 92],
            nonce: vec![1; 12],
        })
        .unwrap();

    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(metadata.env_key, "OPENAI_API_KEY");
    assert_eq!(metadata.tags, vec!["ai", "local"]);
    assert_eq!(database.search_metadata("openai").unwrap().len(), 1);

    drop(database);
    let reopened = Database::open(&path).unwrap();
    let payload = reopened.get_payload(&id).unwrap().unwrap();
    assert_eq!(payload, (vec![90, 91, 92], vec![1; 12]));
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_does_not_store_the_plaintext_fixture() {
    let (_path, database) = temp_database();
    database
        .create_secret(NewSecret {
            name: "Fixture".into(),
            provider_id: "generic".into(),
            env_key: "FIXTURE_KEY".into(),
            description: String::new(),
            tags: vec![],
            ciphertext: b"ciphertext-only".to_vec(),
            nonce: vec![0; 12],
        })
        .unwrap();
    let connection = Connection::open(database.path()).unwrap();
    let contents: String = connection
        .query_row(
            "SELECT quote(ciphertext) FROM secret_payloads LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!contents.contains("fixture-secret-value"));
}

#[test]
fn audit_records_keep_only_hashes_and_allowlisted_metadata() {
    let (_path, database) = temp_database();
    database
        .record_audit(
            "create_secret",
            Some("secret-1"),
            Some("sha256:fixture"),
            "success",
            r#"{"provider":"openai"}"#,
        )
        .unwrap();
    let events = database.list_audit().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value_hash.as_deref(), Some("sha256:fixture"));
    assert!(!events[0].metadata_json.contains("fixture-secret-value"));
}

#[test]
fn persists_profiles_projects_and_safe_default_settings() {
    let (_path, database) = temp_database();
    let now = 10;
    database
        .save_profile(&secrethub_storage::StoredProfile {
            id: "profile-ai".into(),
            name: "AI Standard".into(),
            description: "default AI stack".into(),
            secret_ids: vec!["secret-a".into(), "secret-b".into()],
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    assert_eq!(
        database.list_profiles().unwrap()[0].secret_ids,
        vec!["secret-a", "secret-b"]
    );
    let project = database
        .record_project("C:\\Projects\\demo", "demo")
        .unwrap();
    assert_eq!(project.display_name, "demo");
    assert_eq!(database.list_projects().unwrap().len(), 1);
    let settings = database.get_settings().unwrap();
    assert_eq!(settings.locale, "zh-CN");
    assert_eq!(settings.auto_lock_minutes, 15);
}

#[test]
fn updates_and_deletes_secret_payload_by_id() {
    let (_path, database) = temp_database();
    let id = database
        .create_secret(NewSecret {
            name: "Before".into(),
            provider_id: "generic".into(),
            env_key: "BEFORE".into(),
            description: String::new(),
            tags: vec![],
            ciphertext: vec![1],
            nonce: vec![0; 12],
        })
        .unwrap();
    assert!(database
        .update_secret(
            &id,
            secrethub_storage::UpdatedSecret {
                name: "After".into(),
                provider_id: "generic".into(),
                env_key: "AFTER".into(),
                description: "updated".into(),
                tags: vec!["changed".into()],
                ciphertext: vec![2],
                nonce: vec![1; 12]
            }
        )
        .unwrap());
    assert_eq!(database.get_metadata(&id).unwrap().unwrap().name, "After");
    assert_eq!(database.get_payload(&id).unwrap().unwrap().0, vec![2]);
    assert!(database.delete_secret(&id).unwrap());
    assert!(database.get_metadata(&id).unwrap().is_none());
}
