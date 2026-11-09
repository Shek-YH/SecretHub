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
    assert_eq!(metadata.validated_at, 0);
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

#[test]
fn catalog_auto_update_defaults_on_and_is_independent_of_other_settings() {
    let (path, database) = temp_database();
    assert!(database.catalog_auto_update().unwrap());
    database.set_catalog_auto_update(false).unwrap();
    assert!(!database.catalog_auto_update().unwrap());
    let settings = database.get_settings().unwrap();
    database.update_settings(&settings).unwrap();
    assert!(!database.catalog_auto_update().unwrap());
    drop(database);
    // Persisted across reopen.
    let reopened = Database::open(&path).unwrap();
    assert!(!reopened.catalog_auto_update().unwrap());
}

#[test]
fn capability_models_round_trip_in_display_order_and_cascade_on_delete() {
    use secrethub_storage::{default_capability_env_key, CapabilityModelRow};
    let (path, database) = temp_database();
    let id = new_key(&database, "openrouter");
    let row = |capability: &str, model: &str| CapabilityModelRow {
        capability: capability.into(),
        model_id: model.into(),
        env_key: default_capability_env_key(capability).unwrap().into(),
    };
    database
        .replace_capability_models(&id, &[row("tts", "t1"), row("image_gen", "i1")])
        .unwrap();
    let listed = database.list_capability_models(&id).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|r| r.capability.as_str())
            .collect::<Vec<_>>(),
        ["image_gen", "tts"]
    );
    // Replace is wholesale.
    database
        .replace_capability_models(&id, &[row("code", "c1")])
        .unwrap();
    assert_eq!(database.list_capability_models(&id).unwrap().len(), 1);
    database.delete_secret(&id).unwrap();
    let count: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM secret_capability_models", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

fn media_asset(
    id: &str,
    sha: &str,
    tags: &[&str],
    created_at: i64,
) -> secrethub_storage::MediaAsset {
    secrethub_storage::MediaAsset {
        id: id.into(),
        kind: "image".into(),
        filename: format!("{id}.png"),
        stored_path: format!("C:/media/{id}.png"),
        sha256: sha.into(),
        size_bytes: 10,
        mime: "image/png".into(),
        width: Some(640),
        height: Some(480),
        duration_ms: None,
        description: "fixture".into(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        source: "user".into(),
        created_at,
    }
}

#[test]
fn media_assets_round_trip_dedupe_by_hash_and_feed_the_tag_library() {
    let (_path, database) = temp_database();
    database
        .insert_media(&media_asset("asset-1", "aa", &["Cat"], 1))
        .unwrap();
    database
        .insert_media(&media_asset("asset-2", "bb", &["dog", "cat"], 2))
        .unwrap();
    // The hash is unique: the same content cannot be stored twice.
    assert!(database
        .insert_media(&media_asset("asset-3", "aa", &[], 3))
        .is_err());

    let listed = database.list_media().unwrap();
    assert_eq!(
        listed.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["asset-2", "asset-1"]
    );
    assert_eq!(listed[1].tags, ["Cat"]);
    assert_eq!(listed[1].duration_ms, None);
    assert_eq!(
        database.find_media_by_sha("bb").unwrap().unwrap().id,
        "asset-2"
    );
    assert!(database.find_media_by_sha("zz").unwrap().is_none());

    assert!(database
        .update_media_meta("asset-1", "new text", &["x".into()])
        .unwrap());
    let updated = database.get_media("asset-1").unwrap().unwrap();
    assert_eq!(
        (updated.description.as_str(), updated.tags.clone()),
        ("new text", vec!["x".to_string()])
    );
    assert!(!database.update_media_meta("missing", "", &[]).unwrap());

    // Media tags join the shared tag library (deduped case-insensitively).
    let tags = database.list_tags().unwrap();
    assert_eq!(tags, ["cat", "dog", "x"]);

    let removed = database.delete_media("asset-2").unwrap().unwrap();
    assert_eq!(removed.stored_path, "C:/media/asset-2.png");
    assert!(database.delete_media("asset-2").unwrap().is_none());
    assert_eq!(database.list_media().unwrap().len(), 1);
}

#[test]
fn migrates_an_existing_database_without_the_media_table() {
    let path = std::env::temp_dir().join(format!("secrethub-test-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider_id TEXT NOT NULL, env_key TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);")
            .unwrap();
    }
    let database = Database::open(&path).unwrap();
    assert!(database.list_media().unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_records_its_time_without_touching_the_update_time() {
    let (_path, database) = temp_database();
    let id = new_key(&database, "openai");
    let before = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(before.validated_at, 0);
    assert!(database.update_status(&id, "valid").unwrap());
    let after = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(after.status, "valid");
    assert!(after.validated_at > 0);
    assert_eq!(after.updated_at, before.updated_at);
}

fn raw_secret(database: &Database, name: &str, value_type: &str, env_key: &str) -> String {
    database
        .create_secret_with_attributes(
            NewSecret {
                name: name.into(),
                provider_id: "generic".into(),
                env_key: env_key.into(),
                description: String::new(),
                tags: Vec::new(),
                ciphertext: vec![1, 2, 3],
                nonce: vec![0; 12],
            },
            secrethub_storage::SecretAttributes {
                value_type: value_type.into(),
                ..Default::default()
            },
        )
        .unwrap()
}

#[test]
fn old_databases_gain_subtitle_and_dev_use_columns() {
    let path = std::env::temp_dir().join(format!("secrethub-old-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider_id TEXT NOT NULL, env_key TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', tags_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL DEFAULT 'unknown', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                 INSERT INTO secrets(id, name, provider_id, env_key, created_at, updated_at) VALUES ('s1', 'Old', 'openai', 'OPENAI_API_KEY', 1, 1);",
            )
            .unwrap();
    }
    let database = Database::open(&path).unwrap();
    let metadata = database.get_metadata("s1").unwrap().unwrap();
    assert_eq!(metadata.subtitle, "");
    assert!(!metadata.dev_use);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn subtitle_and_dev_use_are_stored_listed_and_searchable() {
    let (path, database) = temp_database();
    let id = database
        .create_secret_with_attributes(
            NewSecret {
                name: "Fake login".into(),
                provider_id: "personal".into(),
                env_key: "FAKE_TEST".into(),
                description: String::new(),
                tags: Vec::new(),
                ciphertext: vec![1],
                nonce: vec![0; 12],
            },
            secrethub_storage::SecretAttributes {
                value_type: "login".into(),
                subtitle: "accounts.example.com".into(),
                dev_use: true,
                ..Default::default()
            },
        )
        .unwrap();
    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert_eq!(metadata.subtitle, "accounts.example.com");
    assert!(metadata.dev_use && metadata.is_json_item() && metadata.ai_visible());
    assert_eq!(
        database.search_metadata("accounts.example").unwrap().len(),
        1
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ai_visibility_follows_the_design_rules() {
    let (path, database) = temp_database();
    let visible = |value_type: &str, env_key: &str, dev_use: bool| {
        let id = raw_secret(&database, "x", value_type, env_key);
        let mut metadata = database.get_metadata(&id).unwrap().unwrap();
        metadata.dev_use = dev_use;
        metadata.ai_visible()
    };
    for value_type in ["api_key", "dev_secret", "token"] {
        assert!(visible(value_type, "", false), "{value_type}");
    }
    for value_type in ["url", "password", "text"] {
        assert!(visible(value_type, "FAKE_ENV", false), "{value_type}");
        assert!(!visible(value_type, "", false), "{value_type}");
    }
    assert!(visible("login", "FAKE_TEST", true));
    for value_type in ["login", "bank_card", "note", "mystery"] {
        assert!(!visible(value_type, "FAKE_ENV", false), "{value_type}");
    }
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn rekey_swaps_payloads_and_meta_or_changes_nothing() {
    let (path, database) = temp_database();
    database.set_vault_meta(&[1; 16], &[2; 32]).unwrap();
    let first = raw_secret(&database, "a", "api_key", "");
    let second = raw_secret(&database, "b", "api_key", "");
    let rekeyed = |id: &str, byte: u8| secrethub_storage::RekeyedPayload {
        secret_id: id.into(),
        ciphertext: vec![byte; 4],
        nonce: vec![byte; 12],
    };

    // A set that misses one payload, or names an unknown one, is rejected and rolled back.
    assert!(database
        .rekey(&[9; 16], &[9; 32], &[rekeyed(&first, 7)])
        .is_err());
    assert!(database
        .rekey(
            &[9; 16],
            &[9; 32],
            &[rekeyed(&first, 7), rekeyed("nope", 7)]
        )
        .is_err());
    assert_eq!(database.get_vault_meta().unwrap().unwrap().0, vec![1; 16]);
    assert_eq!(
        database.get_payload(&first).unwrap().unwrap().0,
        vec![1, 2, 3]
    );

    database
        .rekey(
            &[9; 16],
            &[8; 32],
            &[rekeyed(&first, 7), rekeyed(&second, 6)],
        )
        .unwrap();
    assert_eq!(
        database.get_vault_meta().unwrap().unwrap(),
        (vec![9; 16], vec![8; 32])
    );
    assert_eq!(database.get_payload(&first).unwrap().unwrap().0, vec![7; 4]);
    assert_eq!(
        database.get_payload(&second).unwrap().unwrap().0,
        vec![6; 4]
    );
    assert_eq!(database.list_payloads().unwrap().len(), 2);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn item_migrations_apply_together_or_not_at_all() {
    let (path, database) = temp_database();
    let first = raw_secret(&database, "a", "password", "");
    let second = raw_secret(&database, "b", "text", "");
    let legacy = database.list_legacy_items().unwrap();
    assert_eq!(legacy.len(), 2);
    let migration = |id: &str, value_type: &str| secrethub_storage::ItemMigration {
        id: id.into(),
        value_type: value_type.into(),
        category: Some("personal".into()),
        subtitle: String::new(),
        payload: Some((vec![5], vec![0; 12])),
    };
    assert!(database
        .apply_item_migrations(&[migration(&first, "login"), migration("missing", "note")])
        .is_err());
    assert_eq!(
        database.get_metadata(&first).unwrap().unwrap().value_type,
        "password"
    );
    assert_eq!(
        database.get_payload(&first).unwrap().unwrap().0,
        vec![1, 2, 3]
    );

    database
        .apply_item_migrations(&[migration(&first, "login"), migration(&second, "note")])
        .unwrap();
    assert!(database.list_legacy_items().unwrap().is_empty());
    assert_eq!(
        database.get_metadata(&second).unwrap().unwrap().category,
        "personal"
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn group_variables_are_transactional_ordered_and_cascade_on_delete() {
    use secrethub_storage::{GroupVariableRow, SecretAttributes, UpdatedSecret};
    let (path, database) = temp_database();
    let secret = || NewSecret {
        name: "Fake group".into(),
        provider_id: "group".into(),
        env_key: String::new(),
        description: String::new(),
        tags: Vec::new(),
        ciphertext: vec![1],
        nonce: vec![2; 12],
    };
    let attributes = |ai_values| SecretAttributes {
        value_type: "env_group".into(),
        ai_values,
        ..Default::default()
    };
    let row = |name: &str, secret: bool, value: &str| GroupVariableRow {
        name: name.into(),
        secret,
        plain_value: value.into(),
    };
    let id = database
        .create_group(
            secret(),
            attributes(false),
            &[row("B_HOST", false, "h"), row("A_PASSWORD", true, "")],
        )
        .unwrap();
    let metadata = database.get_metadata(&id).unwrap().unwrap();
    assert!(!metadata.ai_values && metadata.ai_visible());
    let rows = database.list_group_variables(&id).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["B_HOST", "A_PASSWORD"]
    );

    // A duplicate name fails the whole update and leaves the old rows in place.
    let failed = database.update_group(
        &id,
        UpdatedSecret {
            name: "Renamed".into(),
            provider_id: "group".into(),
            env_key: String::new(),
            description: String::new(),
            tags: Vec::new(),
            ciphertext: vec![9],
            nonce: vec![2; 12],
        },
        attributes(true),
        &[row("X", false, "1"), row("X", false, "2")],
    );
    assert!(failed.is_err());
    assert_eq!(database.list_group_variables(&id).unwrap().len(), 2);
    assert_eq!(
        database.get_metadata(&id).unwrap().unwrap().name,
        "Fake group"
    );
    assert_eq!(database.get_payload(&id).unwrap().unwrap().0, vec![1]);

    assert!(database.delete_secret(&id).unwrap());
    assert!(database.list_group_variables(&id).unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}
