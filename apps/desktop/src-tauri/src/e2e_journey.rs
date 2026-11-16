//! End-to-end "user journey" test: a fake credentials folder shaped like a real user's is
//! scanned, reviewed, imported, re-imported, changed, exported and re-keyed, all against a real
//! vault in a temporary directory. Every value below is an obvious fake; no real data is read.
//!
//! When `SECRETHUB_E2E_OUT` is set, the final vault database is copied there so the MCP journey
//! script (`apps/mcp/tests/import-journey.mjs`) can check what an AI client would see.

use crate::export::{self, ExportRequestV2, ExportSelection};
use crate::import_flow::{
    apply, review, scan_session, ImportDecisions, ImportOutcome, ItemDecision, ReviewDto,
    ReviewItem,
};
use secrethub_core::VaultService;
use secrethub_providers::ProviderDefinition;
use secrethub_storage::{Database, SecretMetadata};
use std::fs;
use std::path::{Path, PathBuf};

const MASTER: &str = "fake-master-password-one";
const NEW_MASTER: &str = "fake-master-password-two";

pub(crate) const YC: &str = "野草云";
pub(crate) const ALI: &str = "阿里宝塔";
pub(crate) const TX: &str = "腾讯宝塔";
const TOPUP: &str = "充值";

fn key(tag: &str) -> String {
    format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEVAL-{tag}-LINE1\nFAKEVAL-{tag}-LINE2\n-----END OPENSSH PRIVATE KEY-----"
    )
}

// Secret values that must never show up in a review, an outcome, an audit trail or an AI view.
const YC_PASSWORD: &str = "fakeval-yc-panel-password";
const YC_TOKEN: &str = "fakeval-yc-panel-token";
const ALI_BT_SK: &str = "fakeval-ali-bt-sk";
const ALI_DOLLAR_PASSWORD: &str = "fakeval$Pa55${HOME}w$$rd";
const ALI_DASH: &str = "sk-fakeval-dashscope";
const ALI_RUNNINGHUB_KEY: &str = "fakeval-runninghub-key";

fn registry() -> Vec<ProviderDefinition> {
    crate::catalog::effective_registry()
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The fake credentials folder; returns its root.
pub(crate) fn build_fixture(base: &Path) -> PathBuf {
    let root = base.join("资料");

    // 野草云: complete server + panel set, a key file identical to the one in .env.
    let yc = root.join(YC);
    write(
        &yc.join(".env"),
        &format!(
            "YECAOYUN_HOST=fakeval-yc-host.invalid\nYECAOYUN_PORT=22\nYECAOYUN_USER=fakeval_user\n\
             YECAOYUN_SSH_PRIVATE_KEY=\"{}\"\nYECAOYUN_SSH_PUBLIC_KEY=ssh-ed25519 FAKEVALPUBLICKEY fake@host\n\
             BT_PANEL_URL=https://fakeval-yc.invalid:8888/fakeval\nBT_PANEL_HOST=fakeval-yc-host.invalid\n\
             BT_PANEL_PORT=8888\nBT_PANEL_USERNAME=fakeval_admin\nBT_PANEL_PASSWORD={YC_PASSWORD}\n\
             BT_PANEL_API_TOKEN={YC_TOKEN}\nBT_PANEL_API_WHITELIST=198.51.100.1\n\
             BT_PANEL_API_BASE=https://fakeval-yc.invalid:8888/api\nYECAOYUN_CONTROL_API_STATUS=enabled\n",
            key("YC")
        ),
    );
    write(&yc.join("id_ed25519"), &format!("{}\n", key("YC")));
    write(
        &yc.join("id_ed25519.pub"),
        "ssh-ed25519 FAKEVALPUBLICKEY fake@host\n",
    );
    write(&yc.join("bt-api.json"), "");
    write(&yc.join("bt-install.log"), "");

    // 阿里宝塔: duplicate private key, a provider key, a `$` password, free text, other files.
    let ali = root.join(ALI);
    write(
        &ali.join(".env"),
        &format!(
            "BT_PANEL_URL=https://fakeval-ali.invalid:8888/fakeval\nBT_API_SK={ALI_BT_SK}\n\
             SSH_HOST=fakeval-ali-host.invalid\nSSH_PORT=22\nSSH_USERNAME=fakeval_root\n\
             SSH_PASSWORD={ALI_DOLLAR_PASSWORD}\nSSH_PRIVATE_KEY=\"{}\"\n\
             free text line one fakeval\nSSH_PRIVATE_KEY=\"{}\"\nDASHSCOPE_API_KEY={ALI_DASH}\n\
             ALIYUN_OSS_BUCKET=fakeval-bucket\nALIYUN_OSS_REGION=oss-cn-fakeval\n\
             RUNNINGHUB_BASE_URL=https://fakeval-runninghub.invalid\nRUNNINGHUB_WORKFLOW_ID=fakeval-wf\n\
             RUNNINGHUB_API_KEY={ALI_RUNNINGHUB_KEY}\nfree text line two fakeval\n",
            key("ALI-ENV-1"),
            key("ALI-ENV-2")
        ),
    );
    write(
        &ali.join("8.0.0.1_id_ed25519"),
        &format!("{}\n", key("ALI-FILE")),
    );
    write(&ali.join("API.txt"), "");
    write(&ali.join("sales.csv"), "city,count\nfakeval-city,3\n");
    write(&ali.join("memo.txt"), "fakeval memo about the panel\n");

    // 腾讯宝塔: nearly all free text, one lower-case variable, a key file.
    let tx = root.join(TX);
    write(
        &tx.join(".env"),
        "腾讯这台机器暂时没有开放面板 fakeval\nmodel=xxx\n备注：以后再补 fakeval\n",
    );
    write(&tx.join("id_rsa"), &format!("{}\n", key("TX")));

    // 充值: a single line of text.
    write(&root.join(TOPUP).join(".env"), "充值记录见微信 fakeval\n");
    root
}

fn open_vault(db: &Path) -> VaultService {
    VaultService::open(Database::open(db).unwrap()).unwrap()
}

fn scan(root: &Path, vault: &VaultService) -> (crate::import_flow::Session, ReviewDto) {
    let session = scan_session(&[root.to_string_lossy().into_owned()], None, &registry()).unwrap();
    let dto = review(vault, &session, "s", &registry()).unwrap();
    (session, dto)
}

fn find<'a>(dto: &'a ReviewDto, kind: &str, name: &str) -> &'a ReviewItem {
    dto.items
        .iter()
        .find(|item| item.kind == kind && item.name == name)
        .unwrap_or_else(|| panic!("missing {kind} {name}"))
}

fn metadata(vault: &VaultService) -> Vec<SecretMetadata> {
    vault.list_metadata().unwrap()
}

fn group_id(vault: &VaultService, name: &str) -> String {
    metadata(vault)
        .into_iter()
        .find(|item| item.value_type == "env_group" && item.name == name)
        .unwrap_or_else(|| panic!("no stored group {name}"))
        .id
}

fn values(vault: &VaultService, id: &str) -> Vec<(String, String, bool)> {
    vault.group_values(id, None).unwrap()
}

fn value_of(vault: &VaultService, group: &str, name: &str) -> String {
    let id = group_id(vault, group);
    values(vault, &id)
        .into_iter()
        .find(|(n, _, _)| n == name)
        .unwrap_or_else(|| panic!("{group} has no {name}"))
        .1
}

fn names_of(item: &ReviewItem) -> Vec<&str> {
    item.variables.iter().map(|v| v.name.as_str()).collect()
}

fn skipped_names(outcome: &ImportOutcome) -> Vec<&str> {
    outcome.skipped.iter().map(String::as_str).collect()
}

fn assert_no_fake_values(label: &str, text: &str) {
    assert!(
        !text.to_lowercase().contains("fakeval"),
        "{label} leaked a fake value"
    );
}

#[test]
fn user_journey_import_export_rekey() {
    let work = tempfile::tempdir().unwrap();
    let root = build_fixture(work.path());
    let db_path = work.path().join("vault.sqlite");
    let mut vault = open_vault(&db_path);
    vault.setup_master(MASTER).unwrap();

    // ---- a. scan the whole folder and check the value-free review
    let (session, dto) = scan(&root, &vault);
    let groups: Vec<&str> = dto
        .items
        .iter()
        .filter(|i| i.kind == "group")
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(groups.len(), 4, "groups: {groups:?}");
    for name in [YC, ALI, TX, TOPUP] {
        assert!(groups.contains(&name), "missing group {name}");
        assert_eq!(find(&dto, "group", name).dedupe.state, "new");
    }

    // The provider key is offered as a split-off API key (qwen), taken out of the group.
    let ali = find(&dto, "group", ALI);
    let dash = ali
        .variables
        .iter()
        .find(|v| v.name == "DASHSCOPE_API_KEY")
        .unwrap();
    let api_item = dash
        .api_key_item
        .clone()
        .expect("DASHSCOPE_API_KEY is split");
    let api = dto.items.iter().find(|i| i.id == api_item).unwrap();
    assert_eq!(api.kind, "api_key");
    assert_eq!(api.split_from.as_deref(), Some(ali.id.as_str()));
    assert_eq!(
        dto.items.iter().filter(|i| i.kind == "api_key").count(),
        1,
        "only the DashScope key is a provider key"
    );

    // The duplicated SSH_PRIVATE_KEY is flagged; the differing key file becomes SSH_PRIVATE_KEY_2.
    let dup = ali
        .variables
        .iter()
        .find(|v| v.name == "SSH_PRIVATE_KEY")
        .unwrap();
    assert!(dup.values == 2 && dup.secret);
    let second = ali
        .variables
        .iter()
        .find(|v| v.name == "SSH_PRIVATE_KEY_2")
        .expect("the key file with a different key is kept under a new name");
    assert!(second.conflict && second.secret);

    // The Yecaoyun key file equals the key in .env, so it adds nothing.
    let yc = find(&dto, "group", YC);
    assert!(!names_of(yc).contains(&"SSH_PRIVATE_KEY"));
    assert!(names_of(yc).contains(&"YECAOYUN_SSH_PRIVATE_KEY"));

    // Empty files are reported as skipped; free text goes to the notes.
    for name in ["bt-api.json", "bt-install.log", "API.txt"] {
        let entry = dto.skipped.iter().find(|s| s.name == name);
        assert_eq!(entry.map(|s| s.reason), Some("empty"), "{name}");
    }
    assert!(ali.has_notes && find(&dto, "group", TX).has_notes);
    assert!(find(&dto, "group", TOPUP).has_notes);
    assert!(names_of(find(&dto, "group", TOPUP)).is_empty());
    assert_no_fake_values("review JSON", &serde_json::to_string(&dto).unwrap());
    drop(session);

    // ---- b. apply with the default choices
    let (session, _) = scan(&root, &vault);
    let outcome = apply(&vault, &session, &ImportDecisions::default()).unwrap();
    assert_eq!(outcome.added.len(), 5, "4 groups + 1 API key: {outcome:?}");
    assert_eq!(outcome.api_key_ids.len(), 1);
    assert!(outcome.updated.is_empty() && outcome.skipped.is_empty());
    assert_no_fake_values("import outcome", &serde_json::to_string(&outcome).unwrap());

    let ali_id = group_id(&vault, ALI);
    let rows = vault.group_variables(&ali_id).unwrap();
    for row in &rows {
        if row.secret {
            assert!(row.plain_value.is_empty(), "{} stored in plain", row.name);
        }
    }
    let plain = |name: &str| {
        rows.iter()
            .find(|r| r.name == name)
            .unwrap()
            .plain_value
            .clone()
    };
    assert_eq!(plain("SSH_HOST"), "fakeval-ali-host.invalid");
    assert_eq!(plain("ALIYUN_OSS_BUCKET"), "fakeval-bucket");
    for secret in [
        "SSH_PASSWORD",
        "SSH_PRIVATE_KEY",
        "SSH_PRIVATE_KEY_2",
        "BT_API_SK",
        "RUNNINGHUB_API_KEY",
    ] {
        assert!(
            rows.iter().find(|r| r.name == secret).unwrap().secret,
            "{secret} should be secret"
        );
    }
    assert!(
        !rows.iter().any(|r| r.name == "DASHSCOPE_API_KEY"),
        "the provider key left the group"
    );
    // The `$` password is stored verbatim (no variable expansion).
    assert_eq!(value_of(&vault, ALI, "SSH_PASSWORD"), ALI_DOLLAR_PASSWORD);
    assert_eq!(value_of(&vault, ALI, "BT_API_SK"), ALI_BT_SK);
    // Last duplicate wins; the file key is kept as the _2 variable.
    assert_eq!(value_of(&vault, ALI, "SSH_PRIVATE_KEY"), key("ALI-ENV-2"));
    assert_eq!(
        value_of(&vault, ALI, "SSH_PRIVATE_KEY_2").trim_end(),
        key("ALI-FILE")
    );
    assert_eq!(value_of(&vault, YC, "BT_PANEL_API_TOKEN"), YC_TOKEN);
    assert_eq!(value_of(&vault, YC, "YECAOYUN_SSH_PRIVATE_KEY"), key("YC"));

    // The split-off key: provider qwen, env name kept, value decrypts.
    let key_meta = metadata(&vault)
        .into_iter()
        .find(|i| i.id == outcome.api_key_ids[0])
        .unwrap();
    assert_eq!(key_meta.provider_id, "qwen");
    assert_eq!(key_meta.env_key, "DASHSCOPE_API_KEY");
    assert_eq!(vault.read_secret(&key_meta.id).unwrap(), ALI_DASH);

    // Notes can be read back.
    let notes = vault.group_notes(&ali_id).unwrap();
    assert!(notes.contains("free text line one fakeval") && notes.contains("memo.txt"));
    assert!(notes.contains("sales.csv") && notes.contains("fakeval-city"));
    let tx_id = group_id(&vault, TX);
    assert!(vault
        .group_notes(&tx_id)
        .unwrap()
        .contains("腾讯这台机器暂时没有开放面板"));
    assert_eq!(value_of(&vault, TX, "model"), "xxx");
    let topup_id = group_id(&vault, TOPUP);
    assert!(vault
        .group_notes(&topup_id)
        .unwrap()
        .contains("充值记录见微信"));

    // What the UI may show without a reveal carries no secret value.
    let public = serde_json::to_string(&vault.group_public(&ali_id).unwrap()).unwrap();
    for secret in [ALI_BT_SK, ALI_DOLLAR_PASSWORD, "FAKEVAL-ALI"] {
        assert!(!public.contains(secret), "group_public leaked a secret");
    }
    assert!(public.contains("fakeval-ali-host.invalid"));
    let stored_count = metadata(&vault).len();
    assert_eq!(stored_count, 5);

    // ---- c. import the same folder again: everything is the same and skipped
    let (session, dto) = scan(&root, &vault);
    assert!(
        dto.items.iter().all(|i| i.dedupe.state == "same"),
        "states: {:?}",
        dto.items
            .iter()
            .map(|i| (&i.name, i.dedupe.state))
            .collect::<Vec<_>>()
    );
    let again = apply(&vault, &session, &ImportDecisions::default()).unwrap();
    assert!(again.added.is_empty() && again.updated.is_empty());
    assert_eq!(again.skipped.len(), 5, "{again:?}");
    assert_eq!(metadata(&vault).len(), stored_count);

    // ---- d. change a plain and a secret value in the folder, then re-import
    let yc_env = root.join(YC).join(".env");
    let changed = fs::read_to_string(&yc_env)
        .unwrap()
        .replace(
            "fakeval-yc-host.invalid\nYECAOYUN_PORT",
            "fakeval-yc-newhost.invalid\nYECAOYUN_PORT",
        )
        .replace(YC_PASSWORD, "fakeval-yc-new-password");
    fs::write(&yc_env, changed).unwrap();
    let (session, dto) = scan(&root, &vault);
    let yc_item = find(&dto, "group", YC);
    assert_eq!(yc_item.dedupe.state, "differs");
    assert_eq!(yc_item.dedupe.default, "merge");
    assert_eq!(yc_item.dedupe.options, ["merge", "copy", "skip"]);
    assert!(yc_item.dedupe.changed.contains(&"YECAOYUN_HOST".to_owned()));
    assert!(yc_item
        .dedupe
        .changed
        .contains(&"BT_PANEL_PASSWORD".to_owned()));
    assert_eq!(
        dto.items
            .iter()
            .filter(|i| i.dedupe.state == "differs")
            .count(),
        1
    );
    assert_no_fake_values("diff review JSON", &serde_json::to_string(&dto).unwrap());

    let choose = |action: &str| ImportDecisions {
        items: vec![ItemDecision {
            id: yc_item.id.clone(),
            include: true,
            action: Some(action.to_owned()),
            ..Default::default()
        }],
    };
    // Skip leaves the stored values alone ...
    let skipped = apply(&vault, &session, &choose("skip")).unwrap();
    assert!(skipped_names(&skipped).contains(&YC) && skipped.updated.is_empty());
    assert_eq!(
        value_of(&vault, YC, "YECAOYUN_HOST"),
        "fakeval-yc-host.invalid"
    );
    // ... merge updates them and keeps the rest.
    let merged = apply(&vault, &session, &choose("merge")).unwrap();
    assert_eq!(merged.updated, [YC]);
    assert!(merged.added.is_empty());
    assert_eq!(
        value_of(&vault, YC, "YECAOYUN_HOST"),
        "fakeval-yc-newhost.invalid"
    );
    assert_eq!(
        value_of(&vault, YC, "BT_PANEL_PASSWORD"),
        "fakeval-yc-new-password"
    );
    assert_eq!(value_of(&vault, YC, "BT_PANEL_API_TOKEN"), YC_TOKEN);
    assert_eq!(metadata(&vault).len(), stored_count, "merge adds no items");
    assert_no_fake_values("merge outcome", &serde_json::to_string(&merged).unwrap());

    // ---- e. export a variable subset of one group plus the split-off API key
    let project = tempfile::tempdir().unwrap();
    write(
        &project.path().join(".env"),
        "PORT=3000\nSSH_HOST=old-host.invalid\n",
    );
    let select = |id: &str, variables: Option<&[&str]>| ExportSelection {
        secret_id: id.to_owned(),
        variables: variables.map(|v| v.iter().map(|n| (*n).to_owned()).collect()),
    };
    let request = |selections, skip: &[&str]| ExportRequestV2 {
        directory: project.path().to_string_lossy().into_owned(),
        selections,
        skip_keys: skip.iter().map(|k| (*k).to_owned()).collect(),
    };
    // The same name from two selected groups is an internal conflict and blocks the export.
    let yc_id = group_id(&vault, YC);
    let clash = request(
        vec![
            select(&ali_id, Some(&["BT_PANEL_URL"])),
            select(&yc_id, Some(&["BT_PANEL_URL"])),
        ],
        &[],
    );
    let preview = export::preview(&vault, &clash).unwrap();
    assert!(preview.rows.iter().all(|r| r.status == "conflict"));
    assert!(export::apply(&vault, &clash).is_err());

    let export_request = |skip: &[&str]| {
        request(
            vec![
                select(
                    &ali_id,
                    Some(&["SSH_HOST", "SSH_PORT", "SSH_USERNAME", "SSH_PASSWORD"]),
                ),
                select(&outcome.api_key_ids[0], None),
            ],
            skip,
        )
    };
    let preview = export::preview(&vault, &export_request(&[])).unwrap();
    let status = |key: &str| {
        preview
            .rows
            .iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("no row {key}"))
            .status
    };
    assert_eq!(status("SSH_HOST"), "overwrite");
    for key in [
        "SSH_PORT",
        "SSH_USERNAME",
        "SSH_PASSWORD",
        "DASHSCOPE_API_KEY",
    ] {
        assert_eq!(status(key), "new", "{key}");
    }
    let masked = preview
        .rows
        .iter()
        .find(|r| r.key == "SSH_PASSWORD")
        .unwrap();
    assert!(masked.secret && !masked.display.contains("fakeval"));
    let shown = preview.rows.iter().find(|r| r.key == "SSH_HOST").unwrap();
    assert_eq!(shown.display, "fakeval-ali-host.invalid");
    let preview_json = serde_json::to_string(&preview).unwrap();
    assert!(!preview_json.contains(ALI_DOLLAR_PASSWORD) && !preview_json.contains(ALI_DASH));

    // Keep the project's own SSH_HOST: skip it, write the rest into the existing file.
    let exported = export::apply(&vault, &export_request(&["SSH_HOST"])).unwrap();
    assert_eq!(exported.skipped, ["SSH_HOST"]);
    assert!(exported.written.contains(&"DASHSCOPE_API_KEY".to_owned()));
    assert!(!exported.written.contains(&"SSH_HOST".to_owned()));
    let env_text = fs::read_to_string(project.path().join(".env")).unwrap();
    assert!(env_text.contains("PORT=3000") && env_text.contains("SSH_HOST=old-host.invalid"));
    assert!(!env_text.contains("fakeval-ali-host.invalid"));
    // `$` is escaped so dotenv readers keep it literal; reading the file back yields the original.
    let read_back = secrethub_exporter::parse_env_entries(&env_text).unwrap();
    assert!(read_back
        .iter()
        .any(|entry| entry.key == "SSH_PASSWORD" && entry.value == ALI_DOLLAR_PASSWORD));
    assert!(env_text.contains(&format!("DASHSCOPE_API_KEY=\"{ALI_DASH}\"")));
    assert!(env_text.contains("SSH_PORT=\"22\""));
    let example = fs::read_to_string(project.path().join(".env.example")).unwrap();
    assert!(example.contains("SSH_PASSWORD") && example.contains("DASHSCOPE_API_KEY"));
    assert_no_fake_values(".env.example", &example);
    assert!(!example.contains("old-host.invalid"));
    assert!(fs::read_to_string(project.path().join(".gitignore"))
        .unwrap()
        .lines()
        .any(|line| line.trim() == ".env"));
    assert_eq!(vault.list_projects().unwrap().len(), 1);
    assert_no_fake_values(
        "audit trail",
        &serde_json::to_string(&vault.list_audit().unwrap()).unwrap(),
    );

    // ---- f. change the master password
    vault.change_master_password(MASTER, NEW_MASTER).unwrap();
    drop(vault);
    let mut stale = open_vault(&db_path);
    assert!(stale.unlock(MASTER).is_err(), "old password must fail");
    let mut fresh = open_vault(&db_path);
    fresh.unlock(NEW_MASTER).unwrap();
    assert_eq!(value_of(&fresh, ALI, "SSH_PASSWORD"), ALI_DOLLAR_PASSWORD);
    assert_eq!(value_of(&fresh, YC, "BT_PANEL_API_TOKEN"), YC_TOKEN);
    assert_eq!(value_of(&fresh, ALI, "SSH_PRIVATE_KEY"), key("ALI-ENV-2"));
    assert_eq!(fresh.read_secret(&key_meta.id).unwrap(), ALI_DASH);
    drop((stale, fresh));

    // ---- g. hand the finished database to the MCP journey script
    if let Ok(out) = std::env::var("SECRETHUB_E2E_OUT") {
        Database::open(&db_path).unwrap().checkpoint().unwrap();
        let out = PathBuf::from(out);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(&db_path, &out).unwrap();
    }
}

/// Regression: values containing `$` must survive export, a dotenv read and a second merge.
#[test]
fn dollar_values_round_trip() {
    use secrethub_exporter::{merge_env, parse_env_entries, render_env, EnvEntry};
    let entries = [EnvEntry::new("SSH_PASSWORD", ALI_DOLLAR_PASSWORD)];
    let rendered = render_env(&entries).unwrap();
    let parsed = parse_env_entries(&rendered).expect("rendered .env should parse");
    assert_eq!(parsed[0].value, ALI_DOLLAR_PASSWORD);
    merge_env(&rendered, &[EnvEntry::new("OTHER", "x")]).expect("a second export should merge");
}

/// The masked view of the whole fake folder shows structure and harmless values, and not one
/// secret: passwords, tokens, keys and the private key bodies are all placeholders.
#[test]
fn masked_view_of_the_fixture_hides_every_secret() {
    let work = tempfile::tempdir().unwrap();
    let root = build_fixture(work.path());
    let read = crate::scan::read_files(&[root]).unwrap();
    let inspection = crate::mask::Inspection::build(&read.files, read.skipped);
    let view: String = inspection
        .files
        .iter()
        .flat_map(|file| file.lines.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    for secret in [
        YC_PASSWORD,
        YC_TOKEN,
        ALI_BT_SK,
        ALI_DOLLAR_PASSWORD,
        ALI_DASH,
        ALI_RUNNINGHUB_KEY,
        "FAKEVAL-YC-LINE1",
        "FAKEVAL-ALI-ENV-1",
        "FAKEVAL-ALI-FILE",
        "FAKEVAL-TX-LINE",
        "FAKEVALPUBLICKEY",
    ] {
        assert!(!view.contains(secret), "the view leaks {secret}");
    }
    // Structure and harmless values stay readable.
    for visible in [
        "YECAOYUN_HOST=fakeval-yc-host.invalid",
        "YECAOYUN_PORT=22",
        "BT_PANEL_PORT=8888",
        "BT_PANEL_PASSWORD=«S",
        "SSH_HOST=fakeval-ali-host.invalid",
        "腾讯这台机器暂时没有开放面板",
    ] {
        assert!(view.contains(visible), "{visible} missing in\n{view}");
    }
    // The key in the .env and in the key file is one placeholder.
    let keys = view.matches("多行私钥").count();
    assert!(keys >= 4, "private keys appear as placeholders: {keys}");
    assert!(inspection.token_count() < view.matches("«S").count());
}
