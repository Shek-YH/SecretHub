use rusqlite::Connection;
use secrethub_storage::{Database, NewSecret, ProviderModelRow};

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

#[test]
fn persists_secret_classification_scope_and_flags() {
    let (_path, database) = temp_database();
    let id = database
        .create_secret_with_attributes(
            NewSecret {
                name: "Classified".into(),
                provider_id: "openai".into(),
                env_key: "OPENAI_API_KEY".into(),
                description: "note".into(),
                tags: vec!["ai".into()],
                ciphertext: vec![1],
                nonce: vec![0; 12],
            },
            secrethub_storage::SecretAttributes {
                value_type: "token".into(),
                category: "ai".into(),
                scope: "project".into(),
                favorite: true,
                archived: false,
                ..Default::default()
            },
        )
        .unwrap();
    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(
        (metadata.value_type, metadata.category, metadata.scope),
        ("token".into(), "ai".into(), "project".into())
    );
    assert!(metadata.favorite);
    assert!(!metadata.archived);
}

fn new_key(database: &Database, provider: &str) -> String {
    database
        .create_secret(NewSecret {
            name: format!("{provider} key"),
            provider_id: provider.into(),
            env_key: "FIXTURE_KEY".into(),
            description: String::new(),
            tags: vec![],
            ciphertext: vec![1, 2, 3],
            nonce: vec![0; 12],
        })
        .unwrap()
}

fn model_row(id: &str, caps: &[&str], source: &str) -> ProviderModelRow {
    ProviderModelRow {
        model_id: id.into(),
        display_name: String::new(),
        input: vec!["text".into()],
        output: vec!["text".into()],
        capabilities: caps.iter().map(|item| item.to_string()).collect(),
        capability_source: source.into(),
        synced_at: 0,
    }
}

#[test]
fn migrates_an_existing_database_without_model_sync_columns() {
    let path = std::env::temp_dir().join(format!("secrethub-old-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE secrets (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, provider_id TEXT NOT NULL,
                env_key TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
                tags_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL DEFAULT 'unknown',
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO secrets(id, name, provider_id, env_key, created_at, updated_at)
             VALUES ('old-1', 'Old', 'openai', 'OPENAI_API_KEY', 1, 1);",
        )
        .unwrap();
    }
    let database = Database::open(&path).unwrap();
    let metadata = database.get_metadata("old-1").unwrap().unwrap();
    assert_eq!(metadata.models_synced_at, 0);
    assert_eq!(metadata.models_sync_status, "");
    assert!(database.list_provider_models("old-1").unwrap().is_empty());
    drop(database);
    // Re-opening is idempotent.
    Database::open(&path).unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn replaces_models_in_one_transaction_and_keeps_rows_on_failed_status() {
    let (_path, database) = temp_database();
    let id = new_key(&database, "openrouter");
    database
        .replace_provider_models(
            &id,
            &[
                model_row("a", &["chat"], "heuristic"),
                model_row("b", &[], "unknown"),
            ],
            100,
            "ok",
        )
        .unwrap();
    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(
        (
            metadata.models_synced_at,
            metadata.models_sync_status.as_str()
        ),
        (100, "ok")
    );
    assert_eq!(database.list_provider_models(&id).unwrap().len(), 2);

    // Replacing again drops rows from the previous sync.
    database
        .replace_provider_models(&id, &[model_row("c", &[], "unknown")], 200, "ok")
        .unwrap();
    let result =
        database.replace_provider_models(&id, &[model_row("x", &[], "unknown")], 300, "ok");
    assert!(result.is_ok());
    assert_eq!(database.list_provider_models(&id).unwrap()[0].model_id, "x");

    // Failed sync updates only the status columns.
    assert!(database.set_models_sync_status(&id, "failed", 400).unwrap());
    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(
        (
            metadata.models_synced_at,
            metadata.models_sync_status.as_str()
        ),
        (400, "failed")
    );
    assert_eq!(database.list_provider_models(&id).unwrap().len(), 1);
    assert!(!database
        .set_models_sync_status("missing", "failed", 1)
        .unwrap());
}

#[test]
fn replace_rolls_back_when_the_secret_status_update_fails() {
    let (path, database) = temp_database();
    let id = new_key(&database, "openai");
    database
        .replace_provider_models(&id, &[model_row("keep", &[], "unknown")], 1, "ok")
        .unwrap();
    // Force the final UPDATE inside the transaction to fail via a trigger.
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER block_sync BEFORE UPDATE OF models_sync_status ON secrets
             BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .unwrap();
    assert!(database
        .replace_provider_models(&id, &[model_row("new", &[], "unknown")], 2, "ok")
        .is_err());
    let rows = database.list_provider_models(&id).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].model_id, "keep");
}

#[test]
fn deleting_a_secret_cascades_to_its_models() {
    let (path, database) = temp_database();
    let id = new_key(&database, "openai");
    database
        .replace_provider_models(&id, &[model_row("a", &[], "unknown")], 1, "ok")
        .unwrap();
    database.delete_secret(&id).unwrap();
    let count: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM provider_models", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn overrides_win_per_provider_and_model_and_survive_resync() {
    let (_path, database) = temp_database();
    let first = new_key(&database, "openrouter");
    let second = new_key(&database, "openrouter");
    let other = new_key(&database, "openai");
    let rows = [model_row("m1", &["chat"], "heuristic")];
    for id in [&first, &second, &other] {
        database
            .replace_provider_models(id, &rows, 1, "ok")
            .unwrap();
    }
    database
        .upsert_capability_override("openrouter", "m1", &["image_gen".to_owned()], "user")
        .unwrap();
    for id in [&first, &second] {
        let listed = database.list_provider_models(id).unwrap();
        assert_eq!(listed[0].capabilities, vec!["image_gen"]);
        assert_eq!(listed[0].capability_source, "user");
    }
    // Same model id under another provider is untouched.
    let untouched = database.list_provider_models(&other).unwrap();
    assert_eq!(untouched[0].capabilities, vec!["chat"]);
    assert_eq!(untouched[0].capability_source, "heuristic");
    // Re-sync does not drop the override.
    database
        .replace_provider_models(&first, &rows, 2, "ok")
        .unwrap();
    assert_eq!(
        database.list_provider_models(&first).unwrap()[0].capability_source,
        "user"
    );
    // Upsert replaces.
    database
        .upsert_capability_override("openrouter", "m1", &["tts".to_owned()], "ai")
        .unwrap();
    let listed = database.list_provider_models(&first).unwrap();
    assert_eq!(
        (
            listed[0].capabilities.clone(),
            listed[0].capability_source.as_str()
        ),
        (vec!["tts".to_owned()], "ai")
    );
}

#[test]
fn tag_library_unions_dedupes_and_validates() {
    let (_path, database) = temp_database();
    database
        .create_secret(NewSecret {
            name: "A".into(),
            provider_id: "generic".into(),
            env_key: "A_KEY".into(),
            description: String::new(),
            tags: vec!["Prod".into(), "ai".into()],
            ciphertext: vec![1],
            nonce: vec![0; 12],
        })
        .unwrap();
    assert_eq!(database.create_tag("  zeta ").unwrap(), "zeta");
    assert_eq!(database.create_tag("PROD").unwrap(), "Prod");
    assert_eq!(database.create_tag("ZETA").unwrap(), "zeta");
    assert_eq!(database.create_tag("Beta").unwrap(), "Beta");
    assert_eq!(
        database.list_tags().unwrap(),
        vec!["ai", "Beta", "Prod", "zeta"]
    );

    for invalid in ["", "   ", "a,b", "bad	tag", &"x".repeat(33)] {
        assert!(database.create_tag(invalid).is_err(), "{invalid:?}");
    }
    assert!(database.create_tag(&"x".repeat(32)).is_ok());

    assert!(database.delete_tag("BETA").unwrap());
    assert!(!database.delete_tag("beta").unwrap());
    // Tags still used by a secret keep appearing after library deletion.
    assert!(!database.delete_tag("ai").unwrap());
    assert!(database.list_tags().unwrap().contains(&"ai".to_owned()));
}
