//! Test fixture for the AI understanding test (design 2026-11-10 section 6): the "ideal" plan an
//! AI would submit for the fake credentials folder of `e2e_journey`, applied through the real
//! guard rails and exported to a project, which yields a commented `.env.example`.
//!
//! Generate the example by hand with
//! `SECRETHUB_ORGANIZE_OUT=<dir> cargo test -p secrethub-desktop --lib organize_fixture -- --nocapture`
//! (writes `.env.example`, `masked-view.txt` and `plan.json` into `<dir>`). Everything here is fake.

use crate::e2e_journey::{build_fixture, ALI, TX, YC};
use crate::export::{self, ExportRequestV2, ExportSelection};
use crate::mask::Inspection;
use crate::organize::{apply_plan, PlanItem, PlanValue, PlanVar};
use secrethub_core::VaultService;
use secrethub_storage::Database;
use std::fs;
use std::path::Path;

/// `S<n>` of the placeholder behind `NAME=` in the first file whose name contains `file`.
fn token(view: &Inspection, file: &str, name: &str) -> String {
    let prefix = format!("{name}=«S");
    for masked in view.files.iter().filter(|f| f.name.contains(file)) {
        for line in &masked.lines {
            if let Some(rest) = line.strip_prefix(&prefix) {
                let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
                return format!("S{number}");
            }
        }
    }
    panic!("no placeholder for {name} in {file}");
}

struct Builder<'a> {
    view: &'a Inspection,
    file: &'static str,
}

impl Builder<'_> {
    fn literal(&self, name: &str, text: &str, description: &str) -> PlanVar {
        PlanVar {
            name: name.to_owned(),
            value: PlanValue {
                token: None,
                literal: Some(text.to_owned()),
            },
            secret: None,
            description: description.to_owned(),
        }
    }

    /// `from` is the variable name in the source file.
    fn secret(&self, name: &str, from: &str, description: &str) -> PlanVar {
        PlanVar {
            name: name.to_owned(),
            value: PlanValue {
                token: Some(token(self.view, self.file, from)),
                literal: None,
            },
            secret: None,
            description: description.to_owned(),
        }
    }
}

fn group(name: &str, category: &str, variables: Vec<PlanVar>) -> PlanItem {
    PlanItem {
        kind: "env_group".to_owned(),
        name: name.to_owned(),
        category: Some(category.to_owned()),
        provider_id: None,
        variables,
        notes: String::new(),
    }
}

/// What a careful AI would submit after reading the masked view of the fake folder: one group per
/// server or service, prefixed upper-case names, one sentence of purpose for each variable.
pub(crate) fn ideal_plan(view: &Inspection) -> Vec<PlanItem> {
    let yc = Builder {
        view,
        file: "野草云/.env",
    };
    let ali = Builder {
        view,
        file: "阿里宝塔/.env",
    };
    let qwen = crate::catalog::effective_registry()
        .into_iter()
        .find(|provider| provider.env_keys.first().map(String::as_str) == Some("DASHSCOPE_API_KEY"))
        .expect("the catalog has the DashScope provider");
    vec![
        group(
            &format!("{YC}服务器"),
            "server",
            vec![
                yc.literal(
                    "YC_SSH_HOST",
                    "fakeval-yc-host.invalid",
                    "野草云服务器的 SSH 主机地址",
                ),
                yc.literal("YC_SSH_PORT", "22", "SSH 端口"),
                yc.literal("YC_SSH_USER", "fakeval_user", "SSH 登录用户名"),
                yc.secret(
                    "YC_SSH_PRIVATE_KEY",
                    "YECAOYUN_SSH_PRIVATE_KEY",
                    "SSH 私钥，用于免密登录该服务器",
                ),
                yc.secret(
                    "YC_SSH_PUBLIC_KEY",
                    "YECAOYUN_SSH_PUBLIC_KEY",
                    "SSH 公钥，已写入服务器的 authorized_keys",
                ),
            ],
        ),
        group(
            &format!("{YC}宝塔面板"),
            "server",
            vec![
                yc.literal(
                    "YC_BT_PANEL_URL",
                    "https://fakeval-yc.invalid:8888/fakeval",
                    "宝塔面板的登录地址（含安全入口）",
                ),
                yc.literal(
                    "YC_BT_PANEL_USERNAME",
                    "fakeval_admin",
                    "宝塔面板登录用户名",
                ),
                yc.secret(
                    "YC_BT_PANEL_PASSWORD",
                    "BT_PANEL_PASSWORD",
                    "宝塔面板登录密码",
                ),
                yc.secret(
                    "YC_BT_PANEL_API_TOKEN",
                    "BT_PANEL_API_TOKEN",
                    "宝塔面板 API 接口密钥，调用面板 API 时使用",
                ),
                yc.literal(
                    "YC_BT_PANEL_API_WHITELIST",
                    "198.51.100.1",
                    "允许调用面板 API 的来源 IP",
                ),
                yc.literal(
                    "YC_BT_PANEL_API_BASE",
                    "https://fakeval-yc.invalid:8888/api",
                    "宝塔面板 API 的基础地址",
                ),
            ],
        ),
        group(
            &format!("{ALI}服务器"),
            "server",
            vec![
                ali.literal(
                    "ALI_BT_SSH_HOST",
                    "fakeval-ali-host.invalid",
                    "阿里云宝塔服务器的 SSH 主机地址",
                ),
                ali.literal("ALI_BT_SSH_PORT", "22", "SSH 端口"),
                ali.literal("ALI_BT_SSH_USER", "fakeval_root", "SSH 登录用户名"),
                ali.secret("ALI_BT_SSH_PASSWORD", "SSH_PASSWORD", "SSH 登录密码"),
                ali.secret(
                    "ALI_BT_SSH_PRIVATE_KEY",
                    "SSH_PRIVATE_KEY",
                    "SSH 私钥，用于免密登录该服务器",
                ),
            ],
        ),
        group(
            &format!("{ALI}面板"),
            "server",
            vec![
                ali.literal(
                    "ALI_BT_PANEL_URL",
                    "https://fakeval-ali.invalid:8888/fakeval",
                    "宝塔面板的登录地址（含安全入口）",
                ),
                ali.secret("ALI_BT_PANEL_API_SK", "BT_API_SK", "宝塔面板 API 接口密钥"),
            ],
        ),
        group(
            "阿里云对象存储",
            "storage",
            vec![
                ali.literal("ALI_OSS_BUCKET", "fakeval-bucket", "对象存储桶名称"),
                ali.literal("ALI_OSS_REGION", "oss-cn-fakeval", "对象存储所在地域"),
            ],
        ),
        group(
            "RunningHub 服务",
            "other",
            vec![
                ali.literal(
                    "RUNNINGHUB_BASE_URL",
                    "https://fakeval-runninghub.invalid",
                    "RunningHub 接口基础地址",
                ),
                ali.literal(
                    "RUNNINGHUB_WORKFLOW_ID",
                    "fakeval-wf",
                    "要运行的 RunningHub 工作流 ID",
                ),
                ali.secret(
                    "RUNNINGHUB_API_KEY",
                    "RUNNINGHUB_API_KEY",
                    "RunningHub 接口密钥",
                ),
            ],
        ),
        PlanItem {
            kind: "api_key".to_owned(),
            name: "通义千问".to_owned(),
            category: None,
            provider_id: Some(qwen.id.clone()),
            variables: vec![ali.secret(
                "DASHSCOPE_API_KEY",
                "DASHSCOPE_API_KEY",
                "通义千问（DashScope）接口密钥",
            )],
            notes: String::new(),
        },
        PlanItem {
            kind: "note".to_owned(),
            name: format!("{TX}服务器"),
            category: None,
            provider_id: None,
            variables: Vec::new(),
            notes: "腾讯这台机器暂时没有开放面板，以后再补".to_owned(),
        },
    ]
}

/// Applies the ideal plan to a fresh vault and exports everything into `project`.
pub(crate) fn apply_and_export(
    work: &Path,
    project: &Path,
) -> (Inspection, Vec<PlanItem>, VaultService) {
    let root = build_fixture(work);
    let read = crate::scan::read_files(&[root]).unwrap();
    let view = Inspection::build(&read.files, read.skipped);
    let plan = ideal_plan(&view);
    let mut vault = VaultService::open(Database::open(work.join("vault.sqlite")).unwrap()).unwrap();
    vault.setup_master("fake-master-password-one").unwrap();
    let outcome = apply_plan(
        &vault,
        &view,
        &plan,
        &crate::catalog::effective_registry(),
        "2026-11-10",
    )
    .unwrap();
    assert!(outcome.rejected.is_empty(), "{:?}", outcome.rejected);
    assert_eq!(outcome.counts.created_groups, 6);
    assert_eq!(outcome.counts.other_items, 2);
    fs::create_dir_all(project).unwrap();
    let selections: Vec<ExportSelection> = vault
        .list_metadata()
        .unwrap()
        .into_iter()
        .filter(|item| item.ai_visible())
        .map(|item| ExportSelection {
            secret_id: item.id,
            variables: None,
        })
        .collect();
    export::apply(
        &vault,
        &ExportRequestV2 {
            directory: project.to_string_lossy().into_owned(),
            selections,
            skip_keys: Vec::new(),
        },
    )
    .unwrap();
    (view, plan, vault)
}

#[test]
fn organize_fixture_exports_a_commented_example_without_values() {
    let work = tempfile::tempdir().unwrap();
    let project = work.path().join("project");
    let (view, plan, _vault) = apply_and_export(work.path(), &project);
    let example = fs::read_to_string(project.join(".env.example")).unwrap();
    let env = fs::read_to_string(project.join(".env")).unwrap();

    // Group headers, purpose comments, provider key comment; names are prefixed per group.
    assert!(
        example
            .contains("# 野草云服务器（服务器）\n# 野草云服务器的 SSH 主机地址\nYC_SSH_HOST=\"\""),
        "{example}"
    );
    assert!(
        example.contains("# SSH 登录密码\nALI_BT_SSH_PASSWORD=\"\""),
        "{example}"
    );
    assert!(
        example.contains("# 阿里云对象存储（对象存储）"),
        "{example}"
    );
    assert!(
        example.contains("API Key\nDASHSCOPE_API_KEY=\"\""),
        "{example}"
    );
    assert!(
        example.contains("\n\n# 野草云宝塔面板（服务器）"),
        "groups are separated: {example}"
    );
    // The real .env carries the same comments and the values.
    assert!(
        env.contains("# SSH 登录密码\nALI_BT_SSH_PASSWORD=\""),
        "{env}"
    );
    assert!(
        env.contains("YC_SSH_HOST=\"fakeval-yc-host.invalid\""),
        "{env}"
    );

    // The example holds no value at all; the masked view and the plan hold no secret.
    let secrets = [
        "fakeval-yc-panel-password",
        "fakeval-yc-panel-token",
        "fakeval-ali-bt-sk",
        "fakeval$Pa55",
        "sk-fakeval-dashscope",
        "fakeval-runninghub-key",
        "FAKEVAL-YC-LINE1",
        "FAKEVALPUBLICKEY",
    ];
    for secret in &secrets {
        assert!(!example.contains(secret), "example leaks {secret}");
    }
    assert!(
        !example.contains("fakeval-yc-host"),
        "plain values stay out of the example too"
    );
    let view_text: String = view
        .files
        .iter()
        .flat_map(|f| f.lines.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let plan_text = format!(
        "{:?}",
        plan.iter()
            .flat_map(|item| item
                .variables
                .iter()
                .map(|v| (&v.value.token, &v.description)))
            .collect::<Vec<_>>()
    );
    for secret in &secrets {
        assert!(!view_text.contains(secret), "view leaks {secret}");
        assert!(!plan_text.contains(secret), "plan leaks {secret}");
    }
    // Every variable of every group got a purpose comment.
    let lines: Vec<&str> = example.lines().collect();
    let group_lines: Vec<usize> = (0..lines.len())
        .filter(|&i| {
            ["YC_", "ALI_", "RUNNINGHUB_"]
                .iter()
                .any(|p| lines[i].starts_with(p))
        })
        .collect();
    assert_eq!(group_lines.len(), 5 + 6 + 5 + 2 + 2 + 3, "{example}");
    for i in group_lines {
        assert!(
            lines[i - 1].starts_with("# "),
            "{} has no comment\n{example}",
            lines[i]
        );
    }

    // The finished vault, for the MCP journey (what an AI client sees of the described groups).
    if let Ok(out) = std::env::var("SECRETHUB_ORGANIZE_DB") {
        Database::open(work.path().join("vault.sqlite"))
            .unwrap()
            .checkpoint()
            .unwrap();
        let out = std::path::PathBuf::from(out);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(work.path().join("vault.sqlite"), &out).unwrap();
    }

    if let Ok(out) = std::env::var("SECRETHUB_ORGANIZE_OUT") {
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        fs::write(out.join(".env.example"), &example).unwrap();
        fs::write(out.join("masked-view.txt"), view_text).unwrap();
        let json: Vec<serde_json::Value> = plan
            .iter()
            .map(|item| {
                serde_json::json!({
                    "type": item.kind,
                    "name": item.name,
                    "category": item.category,
                    "providerId": item.provider_id,
                    "notes": item.notes,
                    "variables": item.variables.iter().map(|v| serde_json::json!({
                        "name": v.name,
                        "value": { "token": v.value.token, "literal": v.value.literal },
                        "description": v.description,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        fs::write(
            out.join("plan.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
        println!("organize fixture written to {}", out.display());
    }
}
