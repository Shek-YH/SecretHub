use secrethub_core::{Fields, ItemUpdate, NewItem, NewSecretInput, VaultError, VaultService};
use secrethub_storage::{Database, NewSecret, SecretAttributes};

const PASSWORD: &str = "correct horse battery staple";
const AAD: &[u8] = b"secrethub:secret-payload:v1";

fn temp_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("secrethub-items-{}.sqlite", uuid::Uuid::new_v4()))
}

fn unlocked(path: &std::path::Path) -> VaultService {
    let mut vault = VaultService::open(Database::open(path).unwrap()).unwrap();
    vault.setup_master(PASSWORD).unwrap();
    vault
}

fn fields(pairs: &[(&str, &str)]) -> Fields {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn login(vault: &VaultService) -> String {
    vault
        .create_item(NewItem {
            item_type: "login".into(),
            name: "Fake site".into(),
            fields: fields(&[
                ("website", "https://accounts.example.com/signin"),
                ("username", "fake-user@example.com"),
                ("password", "fake-password"),
                ("totp", "JBSWY3DPEHPK3PXP"),
                ("notes", "fake note"),
            ]),
            tags: vec!["personal-tag".into()],
            favorite: true,
            dev_use: false,
            env_key: String::new(),
        })
        .unwrap()
}

fn update(name: &str, given: &[(&str, &str)], clear: &[&str]) -> ItemUpdate {
    ItemUpdate {
        name: name.into(),
        fields: fields(given),
        clear: clear.iter().map(|key| (*key).to_owned()).collect(),
        tags: Vec::new(),
        favorite: false,
        dev_use: false,
        env_key: String::new(),
    }
}

fn legacy(
    vault: &VaultService,
    name: &str,
    value_type: &str,
    env_key: &str,
    value: &str,
) -> String {
    vault
        .create_secret_with_attributes(
            NewSecretInput {
                name: name.into(),
                provider_id: "generic".into(),
                env_key: env_key.into(),
                description: String::new(),
                tags: Vec::new(),
                value: value.into(),
            },
            SecretAttributes {
                value_type: value_type.into(),
                ..Default::default()
            },
        )
        .unwrap()
}

#[test]
fn login_round_trips_through_encrypted_json_and_keeps_secrets_out_of_plain_columns() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = login(&vault);

    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(metadata.value_type, "login");
    assert_eq!(metadata.category, "personal");
    assert_eq!(metadata.subtitle, "accounts.example.com");
    assert!(metadata.favorite && !metadata.dev_use && !metadata.ai_visible());
    let plain = serde_json::to_string(&metadata).unwrap();
    for hidden in [
        "fake-password",
        "fake-user@example.com",
        "JBSWY3DPEHPK3PXP",
        "fake note",
    ] {
        assert!(!plain.contains(hidden), "{hidden} leaked into metadata");
    }

    let public = vault.item_public_fields(&id).unwrap();
    assert_eq!(public["username"], "fake-user@example.com");
    assert_eq!(public["notes"], "fake note");
    assert!(!public.contains_key("password") && !public.contains_key("totp"));
    assert_eq!(vault.item_field(&id, "password").unwrap(), "fake-password");
    assert_eq!(vault.item_field(&id, "totp").unwrap(), "JBSWY3DPEHPK3PXP");
    assert!(matches!(
        vault.item_field(&id, "pin"),
        Err(VaultError::InvalidField(_))
    ));
    // The raw payload is the versioned JSON document.
    let raw = vault.read_secret(&id).unwrap();
    assert!(raw.contains("\"v\":1") && raw.contains("\"fields\""));

    let audit = serde_json::to_string(&vault.list_audit().unwrap()).unwrap();
    for hidden in [
        "fake-password",
        "fake-user@example.com",
        "accounts.example.com",
    ] {
        assert!(
            !audit.contains(hidden),
            "{hidden} leaked into the audit log"
        );
    }
    assert!(audit.contains("create_item"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn bank_card_and_note_validate_store_and_compute_subtitles() {
    let path = temp_path();
    let vault = unlocked(&path);
    let card = vault
        .create_item(NewItem {
            item_type: "bank_card".into(),
            name: "Fake card".into(),
            fields: fields(&[
                ("bank", "Fake Bank"),
                ("card_number", "4111 1111 1111 1111"),
                ("holder", "FAKE HOLDER"),
                ("expiry", "09/30"),
                ("cvv", "123"),
                ("pin", "654321"),
            ]),
            tags: Vec::new(),
            favorite: false,
            dev_use: false,
            env_key: String::new(),
        })
        .unwrap();
    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == card)
        .unwrap();
    assert_eq!(metadata.subtitle, "Fake Bank · 尾号 1111");
    assert!(!serde_json::to_string(&metadata).unwrap().contains("4111"));
    assert_eq!(
        vault.item_field(&card, "card_number").unwrap(),
        "4111111111111111"
    );
    let public = vault.item_public_fields(&card).unwrap();
    assert_eq!(public["holder"], "FAKE HOLDER");
    assert!(["card_number", "cvv", "pin"]
        .iter()
        .all(|key| !public.contains_key(*key)));

    let note = vault
        .create_item(NewItem {
            item_type: "note".into(),
            name: "Fake note".into(),
            fields: fields(&[("body", "line one\nline two")]),
            tags: Vec::new(),
            favorite: false,
            dev_use: false,
            env_key: String::new(),
        })
        .unwrap();
    assert_eq!(
        vault.item_field(&note, "body").unwrap(),
        "line one\nline two"
    );
    assert!(vault.item_public_fields(&note).unwrap().is_empty());

    // Invalid input is rejected and the error names the field, never the value.
    let bad = vault.create_item(NewItem {
        item_type: "bank_card".into(),
        name: "Bad".into(),
        fields: fields(&[("bank", "Fake Bank"), ("card_number", "4111 not-a-card")]),
        tags: Vec::new(),
        favorite: false,
        dev_use: false,
        env_key: String::new(),
    });
    let message = bad.err().unwrap().to_string();
    assert!(message.contains("card_number") && !message.contains("not-a-card"));
    for (item_type, input) in [
        ("bank_card", fields(&[("card_number", "4111111111111111")])), // bank missing
        ("note", fields(&[("body", "  ")])),
        ("login", fields(&[("password", "fake-password")])), // username missing
        ("login", fields(&[("username", "u"), ("bogus", "x")])),
        ("api_key", fields(&[("value", "x")])),
        ("dev_secret", fields(&[("value", "x")])),
    ] {
        assert!(
            vault
                .create_item(NewItem {
                    item_type: item_type.into(),
                    name: "Bad".into(),
                    fields: input,
                    tags: Vec::new(),
                    favorite: false,
                    dev_use: false,
                    env_key: String::new(),
                })
                .is_err(),
            "{item_type}"
        );
    }
    // The failed creations left nothing behind.
    assert_eq!(vault.list_metadata().unwrap().len(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn update_keeps_unspecified_secrets_and_clear_removes_optional_fields() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = login(&vault);

    // Only plain fields given: password and totp stay as they were.
    vault
        .update_item(
            &id,
            update(
                "Renamed",
                &[
                    ("website", "https://login.example.org"),
                    ("username", "other-user"),
                    ("password", ""),
                ],
                &[],
            ),
        )
        .unwrap();
    assert_eq!(vault.item_field(&id, "password").unwrap(), "fake-password");
    assert_eq!(vault.item_field(&id, "totp").unwrap(), "JBSWY3DPEHPK3PXP");
    let public = vault.item_public_fields(&id).unwrap();
    assert_eq!(public["username"], "other-user");
    assert_eq!(public["website"], "https://login.example.org");
    // notes was not given at all, so it is unchanged too.
    assert_eq!(public["notes"], "fake note");
    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(metadata.name, "Renamed");
    assert_eq!(metadata.subtitle, "login.example.org");
    assert_eq!(metadata.value_type, "login");

    // A new password overwrites; clear removes optional fields; an emptied plain field clears.
    vault
        .update_item(
            &id,
            update(
                "Renamed",
                &[
                    ("username", "other-user"),
                    ("password", "fake-password-2"),
                    ("website", ""),
                ],
                &["totp", "notes"],
            ),
        )
        .unwrap();
    assert_eq!(
        vault.item_field(&id, "password").unwrap(),
        "fake-password-2"
    );
    assert!(vault.item_field(&id, "totp").is_err());
    let public = vault.item_public_fields(&id).unwrap();
    assert!(!public.contains_key("notes") && !public.contains_key("website"));
    assert_eq!(
        vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .find(|item| item.id == id)
            .unwrap()
            .subtitle,
        ""
    );

    // Required fields cannot be cleared; unknown ones and contradictions are rejected, and a
    // failed update leaves the stored item as it was.
    for bad in [
        update("Renamed", &[], &["username"]),
        update("Renamed", &[], &["nope"]),
        update("Renamed", &[("password", "x")], &["password"]),
        update("Renamed", &[("bogus", "x")], &[]),
        update("Renamed", &[("username", "")], &[]),
        update("   ", &[], &[]),
    ] {
        assert!(vault.update_item(&id, bad).is_err());
    }
    assert_eq!(
        vault.item_field(&id, "password").unwrap(),
        "fake-password-2"
    );
    assert_eq!(
        vault.item_public_fields(&id).unwrap()["username"],
        "other-user"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn raw_string_paths_cannot_create_or_rewrite_typed_items() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = login(&vault);
    for value_type in ["login", "bank_card", "note"] {
        assert!(vault
            .create_secret_with_attributes(
                NewSecretInput {
                    name: "x".into(),
                    provider_id: "generic".into(),
                    env_key: String::new(),
                    description: String::new(),
                    tags: Vec::new(),
                    value: "v".into(),
                },
                SecretAttributes {
                    value_type: value_type.into(),
                    ..Default::default()
                },
            )
            .is_err());
    }
    let input = || NewSecretInput {
        name: "x".into(),
        provider_id: "generic".into(),
        env_key: String::new(),
        description: String::new(),
        tags: Vec::new(),
        value: "v".into(),
    };
    assert!(vault.update_secret(&id, input()).is_err());
    assert!(vault
        .update_metadata_with_attributes(&id, input(), SecretAttributes::default())
        .is_err());
    // Typed commands refuse raw-string items in turn.
    let api = legacy(&vault, "key", "api_key", "FAKE_KEY", "fake-key-value");
    assert!(vault.item_field(&api, "password").is_err());
    assert!(vault.update_item(&api, update("x", &[], &[])).is_err());
    assert_eq!(vault.item_field(&id, "password").unwrap(), "fake-password");
    let _ = std::fs::remove_file(path);
}

#[test]
fn dev_secrets_use_their_variable_name_as_subtitle() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = legacy(
        &vault,
        "Token",
        "dev_secret",
        "FAKE_TOKEN",
        "fake-token-value",
    );
    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(metadata.subtitle, "FAKE_TOKEN");
    assert!(metadata.ai_visible());
    vault
        .update_metadata_with_attributes(
            &id,
            NewSecretInput {
                name: "Token".into(),
                provider_id: "generic".into(),
                env_key: "OTHER_TOKEN".into(),
                description: String::new(),
                tags: Vec::new(),
                value: String::new(),
            },
            SecretAttributes {
                value_type: "dev_secret".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let after = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(after.subtitle, "OTHER_TOKEN");
    assert_eq!(vault.read_secret(&id).unwrap(), "fake-token-value");
    let _ = std::fs::remove_file(path);
}

#[test]
fn dev_use_needs_a_valid_prefix_and_only_applies_to_logins() {
    let path = temp_path();
    let vault = unlocked(&path);
    let make = |dev_use: bool, env_key: &str, item_type: &str, input: Fields| {
        vault.create_item(NewItem {
            item_type: item_type.into(),
            name: "Dev".into(),
            fields: input,
            tags: Vec::new(),
            favorite: false,
            dev_use,
            env_key: env_key.into(),
        })
    };
    let login_fields = || fields(&[("username", "fake-user"), ("password", "fake-password")]);
    assert!(make(true, "", "login", login_fields()).is_err());
    assert!(make(true, "lower case", "login", login_fields()).is_err());
    let id = make(true, "DOUYIN_TEST", "login", login_fields()).unwrap();
    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert!(metadata.dev_use && metadata.ai_visible());
    assert_eq!(metadata.env_key, "DOUYIN_TEST");

    // Switching it off clears the prefix; other types never get dev use.
    vault.update_item(&id, update("Dev", &[], &[])).unwrap();
    let off = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert!(!off.dev_use && off.env_key.is_empty() && !off.ai_visible());
    let note = make(true, "FAKE_PREFIX", "note", fields(&[("body", "text")])).unwrap();
    let note_meta = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == note)
        .unwrap();
    assert!(!note_meta.dev_use && note_meta.env_key.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_rows_migrate_per_the_table_and_a_second_run_changes_nothing() {
    let path = temp_path();
    let vault = unlocked(&path);
    let token = legacy(&vault, "tok", "token", "", "fake-token");
    let url_env = legacy(
        &vault,
        "url-env",
        "url",
        "FAKE_URL",
        "https://example.com/a",
    );
    let password_env = legacy(&vault, "pw-env", "password", "FAKE_PW", "fake-password-env");
    let text_env = legacy(&vault, "text-env", "text", "FAKE_TEXT", "fake text env");
    let password = legacy(&vault, "pw", "password", "", "fake-password");
    let text = legacy(&vault, "text", "text", "", "plain fake note");
    let url = legacy(
        &vault,
        "url",
        "url",
        "",
        "https://accounts.example.com/login",
    );
    let api = legacy(&vault, "api", "api_key", "FAKE_API", "fake-api-key");

    assert_eq!(vault.migrate_legacy_items().unwrap(), 7);
    let after = vault.list_metadata().unwrap();
    let get = |id: &str| after.iter().find(|item| item.id == id).unwrap().clone();

    for (id, value, subtitle) in [
        (&token, "fake-token", ""),
        (&url_env, "https://example.com/a", "FAKE_URL"),
        (&password_env, "fake-password-env", "FAKE_PW"),
        (&text_env, "fake text env", "FAKE_TEXT"),
    ] {
        let item = get(id);
        assert_eq!(item.value_type, "dev_secret");
        assert_eq!(item.subtitle, subtitle);
        assert!(item.ai_visible());
        // Raw payload is untouched.
        assert_eq!(vault.read_secret(id).unwrap(), value);
    }
    let password_item = get(&password);
    assert_eq!(password_item.value_type, "login");
    assert_eq!(password_item.category, "personal");
    assert!(!password_item.ai_visible());
    assert_eq!(
        vault.read_item_fields(&password).unwrap(),
        fields(&[("password", "fake-password")])
    );
    assert_eq!(get(&text).value_type, "note");
    assert_eq!(
        vault.read_item_fields(&text).unwrap(),
        fields(&[("body", "plain fake note")])
    );
    let url_item = get(&url);
    assert_eq!(url_item.value_type, "login");
    assert_eq!(url_item.subtitle, "accounts.example.com");
    assert_eq!(
        vault.read_item_fields(&url).unwrap(),
        fields(&[("website", "https://accounts.example.com/login")])
    );
    assert_eq!(get(&api).value_type, "api_key");

    // Idempotent: nothing changes, not even update times or payloads.
    let payloads: Vec<String> = [&password, &text, &url]
        .iter()
        .map(|id| vault.read_secret(id).unwrap())
        .collect();
    assert_eq!(vault.migrate_legacy_items().unwrap(), 0);
    assert_eq!(vault.list_metadata().unwrap(), after);
    let again: Vec<String> = [&password, &text, &url]
        .iter()
        .map(|id| vault.read_secret(id).unwrap())
        .collect();
    assert_eq!(payloads, again);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unlocking_runs_the_migration() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    let id = legacy(&vault, "pw", "password", "", "fake-password");
    vault.lock();
    vault.unlock(PASSWORD).unwrap();
    let item = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(item.value_type, "login");
    assert_eq!(vault.item_field(&id, "password").unwrap(), "fake-password");
    let _ = std::fs::remove_file(path);
}

#[test]
fn raw_string_payloads_in_typed_rows_are_still_readable() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = login(&vault);
    drop(vault);

    // A typed row whose payload is still a raw string (interrupted migration) falls back.
    let database = Database::open(&path).unwrap();
    let (salt, _) = database.get_vault_meta().unwrap().unwrap();
    let key = secrethub_crypto::derive_key(PASSWORD, &salt).unwrap();
    let mut ids = Vec::new();
    for (value_type, raw) in [
        ("login", "fake-password"),
        ("login", "https://example.com"),
        ("note", "just text"),
        ("bank_card", "4111111111111111"),
    ] {
        let encrypted = secrethub_crypto::encrypt(&key, raw.as_bytes(), AAD).unwrap();
        ids.push(
            database
                .create_secret_with_attributes(
                    NewSecret {
                        name: raw.into(),
                        provider_id: "personal".into(),
                        env_key: String::new(),
                        description: String::new(),
                        tags: Vec::new(),
                        ciphertext: encrypted.ciphertext,
                        nonce: encrypted.nonce.to_vec(),
                    },
                    SecretAttributes {
                        value_type: value_type.into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
        );
    }
    let mut vault = VaultService::open(database).unwrap();
    vault.unlock(PASSWORD).unwrap();
    assert_eq!(
        vault.item_field(&ids[0], "password").unwrap(),
        "fake-password"
    );
    assert_eq!(
        vault.item_field(&ids[1], "website").unwrap(),
        "https://example.com"
    );
    assert_eq!(vault.item_field(&ids[2], "body").unwrap(), "just text");
    assert_eq!(
        vault.item_field(&ids[3], "card_number").unwrap(),
        "4111111111111111"
    );
    assert_eq!(vault.item_field(&id, "password").unwrap(), "fake-password");
    let _ = std::fs::remove_file(path);
}

#[test]
fn changing_the_master_password_reencrypts_everything() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    let item = login(&vault);
    let api = legacy(&vault, "api", "api_key", "FAKE_API", "fake-api-key");

    assert!(matches!(
        vault.change_master_password("wrong password!", "new correct password"),
        Err(VaultError::InvalidPassword)
    ));
    assert!(matches!(
        vault.change_master_password(PASSWORD, "short"),
        Err(VaultError::InvalidPassword)
    ));
    vault
        .change_master_password(PASSWORD, "new correct password")
        .unwrap();

    // Records stay readable with the swapped in-memory key.
    assert_eq!(vault.read_secret(&api).unwrap(), "fake-api-key");
    assert_eq!(
        vault.item_field(&item, "password").unwrap(),
        "fake-password"
    );
    // And a freshly created record uses the new key too.
    let later = legacy(&vault, "later", "api_key", "", "fake-later-value");
    assert!(vault
        .list_audit()
        .unwrap()
        .iter()
        .any(|event| event.operation == "master_password_changed"));
    vault.lock();
    assert!(matches!(
        vault.unlock(PASSWORD),
        Err(VaultError::InvalidPassword)
    ));
    drop(vault);

    let mut reopened = VaultService::open(Database::open(&path).unwrap()).unwrap();
    assert!(reopened.unlock(PASSWORD).is_err());
    reopened.unlock("new correct password").unwrap();
    assert_eq!(reopened.read_secret(&api).unwrap(), "fake-api-key");
    assert_eq!(reopened.read_secret(&later).unwrap(), "fake-later-value");
    assert_eq!(
        reopened.item_field(&item, "totp").unwrap(),
        "JBSWY3DPEHPK3PXP"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_failing_password_change_rolls_back_and_the_old_password_keeps_working() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    let item = login(&vault);

    // A row the old key cannot decrypt makes the re-encryption fail before anything is swapped.
    let other = Database::open(&path).unwrap();
    other
        .create_secret(NewSecret {
            name: "corrupt".into(),
            provider_id: "generic".into(),
            env_key: String::new(),
            description: String::new(),
            tags: Vec::new(),
            ciphertext: vec![1, 2, 3, 4],
            nonce: vec![0; 12],
        })
        .unwrap();
    drop(other);

    assert!(vault
        .change_master_password(PASSWORD, "new correct password")
        .is_err());
    // Same session, and a fresh unlock, still use the old password and old data.
    assert_eq!(
        vault.item_field(&item, "password").unwrap(),
        "fake-password"
    );
    vault.lock();
    assert!(vault.unlock("new correct password").is_err());
    vault.unlock(PASSWORD).unwrap();
    assert_eq!(
        vault.item_field(&item, "password").unwrap(),
        "fake-password"
    );
    assert!(!vault
        .list_audit()
        .unwrap()
        .iter()
        .any(|event| event.operation == "master_password_changed"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn password_change_requires_an_unlocked_vault() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    vault.lock();
    assert!(matches!(
        vault.change_master_password(PASSWORD, "new correct password"),
        Err(VaultError::Locked)
    ));
    let _ = std::fs::remove_file(path);
}
