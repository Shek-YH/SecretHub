use secrethub_core::{
    GroupUpdate, NewGroup, NewSecretInput, NewVariable, VariableUpdate, VaultError, VaultService,
};
use secrethub_storage::{Database, SecretAttributes};

const PASSWORD: &str = "correct horse battery staple";
const FAKE_KEY: &str =
    "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----";

fn temp_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("secrethub-groups-{}.sqlite", uuid::Uuid::new_v4()))
}

fn unlocked(path: &std::path::Path) -> VaultService {
    let mut vault = VaultService::open(Database::open(path).unwrap()).unwrap();
    vault.setup_master(PASSWORD).unwrap();
    vault
}

fn variable(name: &str, value: &str, secret: bool) -> NewVariable {
    NewVariable {
        name: name.into(),
        value: value.into(),
        secret,
    }
}

fn group_input(name: &str) -> NewGroup {
    NewGroup {
        name: name.into(),
        category: "server".into(),
        variables: vec![
            variable("SSH_HOST", "203.0.113.7", false),
            variable("SSH_PASSWORD", "fake-ssh-password", true),
            variable("SSH_PRIVATE_KEY", FAKE_KEY, true),
        ],
        notes: "fake note about the server".into(),
        tags: vec!["prod".into()],
        favorite: false,
        ai_values: true,
    }
}

fn keep(name: &str, secret: bool) -> VariableUpdate {
    VariableUpdate {
        name: name.into(),
        secret,
        value: None,
        from: None,
    }
}

fn plain(name: &str, value: &str) -> VariableUpdate {
    VariableUpdate {
        name: name.into(),
        secret: false,
        value: Some(value.into()),
        from: None,
    }
}

fn update(variables: Vec<VariableUpdate>) -> GroupUpdate {
    GroupUpdate {
        name: "Fake server".into(),
        category: "server".into(),
        variables,
        notes: None,
        tags: Vec::new(),
        favorite: false,
        ai_values: true,
    }
}

#[test]
fn storage_round_trip_keeps_plain_values_readable_and_secrets_encrypted() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = new_group(&vault, "Fake server");

    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(metadata.value_type, "env_group");
    assert_eq!(metadata.category, "server");
    assert_eq!(metadata.subtitle, "3 个变量");
    assert!(metadata.env_key.is_empty());
    assert!(metadata.ai_visible() && metadata.is_json_item() && metadata.ai_values);

    // A second connection sees the raw stored form.
    let raw = Database::open(&path).unwrap();
    let rows = raw.list_group_variables(&id).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "SSH_HOST");
    assert_eq!(rows[0].plain_value, "203.0.113.7");
    assert!(rows[1].secret && rows[1].plain_value.is_empty());
    assert!(rows[2].secret && rows[2].plain_value.is_empty());
    let (ciphertext, _) = raw.get_payload(&id).unwrap().unwrap();
    let blob = String::from_utf8_lossy(&ciphertext).into_owned();
    assert!(!blob.contains("fake-ssh-password") && !blob.contains("FAKEFAKEFAKE"));
    assert!(!blob.contains("203.0.113.7"));

    let public = vault.group_public(&id).unwrap();
    assert!(public.has_notes && public.ai_values);
    assert_eq!(public.variables[0].value.as_deref(), Some("203.0.113.7"));
    assert!(public.variables[1].value.is_none() && public.variables[2].value.is_none());
    let serialized = serde_json::to_string(&public).unwrap();
    assert!(!serialized.contains("fake-ssh-password") && !serialized.contains("FAKEFAKEFAKE"));

    assert_eq!(
        vault.group_variable_value(&id, "SSH_PASSWORD").unwrap(),
        "fake-ssh-password"
    );
    assert_eq!(
        vault.group_variable_value(&id, "SSH_PRIVATE_KEY").unwrap(),
        FAKE_KEY
    );
    assert_eq!(
        vault.group_notes(&id).unwrap(),
        "fake note about the server"
    );
    assert!(vault.group_variable_value(&id, "MISSING").is_err());
    let values = vault
        .group_values(&id, Some(&["SSH_PASSWORD".to_owned()]))
        .unwrap();
    assert_eq!(
        values,
        [(
            "SSH_PASSWORD".to_owned(),
            "fake-ssh-password".to_owned(),
            true
        )]
    );
    assert!(vault.group_values(&id, Some(&["NOPE".to_owned()])).is_err());
    assert_eq!(vault.group_values(&id, None).unwrap().len(), 3);

    // Raw-string commands never hand out or rewrite the JSON payload.
    assert!(vault
        .update_secret(
            &id,
            NewSecretInput {
                name: "x".into(),
                provider_id: "generic".into(),
                env_key: String::new(),
                description: String::new(),
                tags: Vec::new(),
                value: "v".into(),
            },
        )
        .is_err());
    assert!(vault
        .list_audit()
        .unwrap()
        .iter()
        .any(|event| event.operation == "create_group"
            && !event.metadata_json.contains("fake-ssh-password")));
    let _ = std::fs::remove_file(path);
}

fn new_group(vault: &VaultService, name: &str) -> String {
    vault.create_group(group_input(name)).unwrap()
}

#[test]
fn update_keeps_secrets_renames_with_from_and_replaces_the_list() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = new_group(&vault, "Fake server");

    let mut change = update(vec![
        plain("SSH_HOST", "198.51.100.9"),
        VariableUpdate {
            name: "ROOT_PASSWORD".into(),
            secret: true,
            value: None,
            from: Some("SSH_PASSWORD".into()),
        },
        keep("SSH_PRIVATE_KEY", true),
        plain("SSH_PORT", "22"),
    ]);
    change.tags = vec!["edited".into()];
    vault.update_group(&id, change).unwrap();

    assert_eq!(
        vault.group_variable_value(&id, "ROOT_PASSWORD").unwrap(),
        "fake-ssh-password"
    );
    assert_eq!(
        vault.group_variable_value(&id, "SSH_PRIVATE_KEY").unwrap(),
        FAKE_KEY
    );
    assert_eq!(
        vault.group_variable_value(&id, "SSH_HOST").unwrap(),
        "198.51.100.9"
    );
    assert!(vault.group_variable_value(&id, "SSH_PASSWORD").is_err());
    // The notes were left out of the update and stay.
    assert_eq!(
        vault.group_notes(&id).unwrap(),
        "fake note about the server"
    );
    let metadata = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(metadata.subtitle, "4 个变量");
    assert_eq!(metadata.tags, ["edited"]);

    // Explicit value replaces a secret; explicit notes replace the notes; ai_values toggles.
    let mut second = update(vec![
        VariableUpdate {
            name: "ROOT_PASSWORD".into(),
            secret: true,
            value: Some("fake-new-password".into()),
            from: None,
        },
        plain("SSH_HOST", ""),
    ]);
    second.notes = Some("changed".into());
    second.ai_values = false;
    vault.update_group(&id, second).unwrap();
    assert_eq!(
        vault.group_variable_value(&id, "ROOT_PASSWORD").unwrap(),
        "fake-new-password"
    );
    assert_eq!(vault.group_variable_value(&id, "SSH_HOST").unwrap(), "");
    assert_eq!(vault.group_notes(&id).unwrap(), "changed");
    assert!(!vault.group_public(&id).unwrap().ai_values);
    assert_eq!(vault.group_variables(&id).unwrap().len(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn update_enforces_plain_values_and_switching_flags() {
    let path = temp_path();
    let vault = unlocked(&path);
    let id = new_group(&vault, "Fake server");

    // A plain variable must carry its value; a secret cannot silently become plain.
    let missing_plain = update(vec![keep("SSH_HOST", false)]);
    assert!(matches!(
        vault.update_group(&id, missing_plain),
        Err(VaultError::InvalidField(name)) if name == "SSH_HOST"
    ));
    assert!(vault
        .update_group(&id, update(vec![keep("SSH_PASSWORD", false)]))
        .is_err());
    // Nothing was written by the failed attempts.
    assert_eq!(vault.group_variables(&id).unwrap().len(), 3);
    assert_eq!(
        vault.group_variable_value(&id, "SSH_PASSWORD").unwrap(),
        "fake-ssh-password"
    );

    // A plain variable can become secret and keeps its value.
    vault
        .update_group(
            &id,
            update(vec![keep("SSH_HOST", true), keep("SSH_PASSWORD", true)]),
        )
        .unwrap();
    let rows = vault.group_variables(&id).unwrap();
    assert!(rows
        .iter()
        .all(|row| row.secret && row.plain_value.is_empty()));
    assert_eq!(
        vault.group_variable_value(&id, "SSH_HOST").unwrap(),
        "203.0.113.7"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn validation_rejects_bad_names_sizes_and_categories() {
    let path = temp_path();
    let vault = unlocked(&path);
    let build = |variables: Vec<NewVariable>, notes: &str, category: &str| NewGroup {
        name: "Fake".into(),
        category: category.into(),
        variables,
        notes: notes.into(),
        tags: Vec::new(),
        favorite: false,
        ai_values: true,
    };
    let ok = |group: NewGroup| vault.create_group(group);

    assert!(ok(build(vec![variable("A", "1", false)], "", "other")).is_ok());
    assert!(ok(build(vec![variable("1A", "1", false)], "", "other")).is_err());
    assert!(ok(build(vec![variable("A-B", "1", false)], "", "other")).is_err());
    assert!(ok(build(
        vec![variable("A", "1", false), variable("A", "2", true)],
        "",
        "other"
    ))
    .is_err());
    let many: Vec<NewVariable> = (0..201)
        .map(|index| variable(&format!("V{index}"), "x", false))
        .collect();
    assert!(ok(build(many, "", "other")).is_err());
    let max: Vec<NewVariable> = (0..200)
        .map(|index| variable(&format!("V{index}"), "x", false))
        .collect();
    assert!(ok(build(max, "", "other")).is_ok());
    assert!(ok(build(
        vec![variable("A", &"x".repeat(64 * 1024 + 1), true)],
        "",
        "other"
    ))
    .is_err());
    assert!(ok(build(
        vec![variable("A", &"x".repeat(64 * 1024), true)],
        "",
        "other"
    ))
    .is_ok());
    assert!(ok(build(vec![], &"n".repeat(100_001), "other")).is_err());
    assert!(ok(build(vec![], &"n".repeat(100_000), "other")).is_ok());
    assert!(ok(build(vec![], "", "personal")).is_err());
    let mut unnamed = build(vec![], "", "other");
    unnamed.name = "  ".into();
    assert!(ok(unnamed).is_err());
    // The error never echoes the value.
    let error = ok(build(
        vec![variable("A", &"x".repeat(70_000), true)],
        "",
        "other",
    ))
    .unwrap_err()
    .to_string();
    assert!(!error.contains("xxxx"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn changing_the_master_password_reencrypts_config_groups() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    let id = new_group(&vault, "Fake server");
    vault
        .change_master_password(PASSWORD, "new correct password")
        .unwrap();
    assert_eq!(
        vault.group_variable_value(&id, "SSH_PASSWORD").unwrap(),
        "fake-ssh-password"
    );
    drop(vault);

    let mut reopened = VaultService::open(Database::open(&path).unwrap()).unwrap();
    assert!(reopened.unlock(PASSWORD).is_err());
    reopened.unlock("new correct password").unwrap();
    assert_eq!(
        reopened
            .group_variable_value(&id, "SSH_PRIVATE_KEY")
            .unwrap(),
        FAKE_KEY
    );
    assert_eq!(
        reopened.group_notes(&id).unwrap(),
        "fake note about the server"
    );
    assert_eq!(
        reopened.group_variable_value(&id, "SSH_HOST").unwrap(),
        "203.0.113.7"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn group_commands_reject_other_items_and_locked_vaults() {
    let path = temp_path();
    let mut vault = unlocked(&path);
    let api = vault
        .create_secret_with_attributes(
            NewSecretInput {
                name: "api".into(),
                provider_id: "generic".into(),
                env_key: "FAKE_API".into(),
                description: String::new(),
                tags: Vec::new(),
                value: "fake-api-key".into(),
            },
            SecretAttributes::default(),
        )
        .unwrap();
    assert!(vault.group_public(&api).is_err());
    assert!(vault.group_notes(&api).is_err());
    assert!(vault.update_group(&api, update(Vec::new())).is_err());
    let id = new_group(&vault, "Fake server");
    vault.lock();
    assert!(matches!(
        vault.group_variable_value(&id, "SSH_PASSWORD"),
        Err(VaultError::Locked)
    ));
    assert!(matches!(vault.group_notes(&id), Err(VaultError::Locked)));
    let _ = std::fs::remove_file(path);
}
