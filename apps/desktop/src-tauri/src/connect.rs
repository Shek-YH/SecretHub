//! One-click "connect AI tools": registers the bundled MCP sidecar with Claude Code, Claude Desktop
//! and Cursor, and copies the bundled skills into the user's Claude skills directory.
//! Nothing here touches the vault or any secret. The filesystem logic takes explicit paths so it can
//! be tested against temp directories.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, Manager};

pub const SKILL_NAMES: [&str; 5] = [
    "secrethub-project-env",
    "secrethub-import",
    "secrethub-model-refresh",
    "secrethub-key-onboarding",
    "secrethub-catalog-maintainer",
];
const SERVER_NAME: &str = "secrethub";
const STATE_FILE: &str = "ai-connect.json";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConnectItem {
    pub id: String,
    /// "connected" | "skipped" | "failed"
    pub status: String,
    pub detail: String,
}

impl ConnectItem {
    fn new(id: &str, status: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            status: status.to_owned(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConnectState {
    connected_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectStatus {
    pub connected_version: Option<String>,
    pub app_version: String,
    pub sidecar_found: bool,
}

// ---------- path helpers ----------

fn sidecar_file_name() -> &'static str {
    if cfg!(windows) {
        "secrethub-mcp.exe"
    } else {
        "secrethub-mcp"
    }
}

/// Installed layout: the sidecar sits next to the main exe. Dev builds fall back to `binaries/`.
fn locate_sidecar() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let beside = exe.parent()?.join(sidecar_file_name());
    if beside.is_file() {
        return Some(beside);
    }
    if cfg!(debug_assertions) {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("secrethub-mcp-x86_64-pc-windows-msvc.exe");
        if dev.is_file() {
            return Some(dev);
        }
    }
    None
}

fn locate_skills(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(resources) = app.path().resource_dir() {
        let bundled = resources.join("skills");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    if cfg!(debug_assertions) {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills");
        if dev.is_dir() {
            return Some(dev);
        }
    }
    None
}

fn find_in_path(
    name: &str,
    path_var: Option<std::ffi::OsString>,
    exts: &[&str],
) -> Option<PathBuf> {
    let path_var = path_var?;
    for dir in env::split_paths(&path_var) {
        for ext in exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_claude() -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    find_in_path("claude", env::var_os("PATH"), exts)
}

// ---------- JSON config merge ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectItemResult {
    Connected(String),
    Skipped(String),
    Failed(String),
}

impl ConnectItemResult {
    fn into_item(self, id: &str) -> ConnectItem {
        match self {
            Self::Connected(detail) => ConnectItem::new(id, "connected", detail),
            Self::Skipped(detail) => ConnectItem::new(id, "skipped", detail),
            Self::Failed(detail) => ConnectItem::new(id, "failed", detail),
        }
    }
}

/// Merge `mcpServers.secrethub = {command, args: []}` into an MCP client config file.
/// Skipped when the parent directory does not exist (client not installed). An existing file is
/// backed up to `<file>.bak` first; unparsable JSON is never overwritten.
pub fn merge_mcp_json(file: &Path, exe: &Path) -> ConnectItemResult {
    let Some(parent) = file.parent() else {
        return ConnectItemResult::Skipped("配置路径无效".to_owned());
    };
    if !parent.is_dir() {
        return ConnectItemResult::Skipped("未检测到该软件（配置目录不存在）".to_owned());
    }
    let existed = file.exists();
    let mut root: Value = if existed {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => return ConnectItemResult::Failed(format!("无法读取配置文件：{error}")),
        };
        let text = text.trim_start_matches('\u{feff}');
        if text.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            match serde_json::from_str(text) {
                Ok(value) => value,
                Err(error) => {
                    return ConnectItemResult::Failed(format!(
                        "配置文件不是有效的 JSON，未做任何修改：{error}"
                    ))
                }
            }
        }
    } else {
        Value::Object(Map::new())
    };
    let Some(object) = root.as_object_mut() else {
        return ConnectItemResult::Failed("配置文件顶层不是对象，未做任何修改".to_owned());
    };
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return ConnectItemResult::Failed("mcpServers 不是对象，未做任何修改".to_owned());
    };
    servers.insert(
        SERVER_NAME.to_owned(),
        json!({ "command": exe.to_string_lossy(), "args": [] }),
    );
    if existed {
        let mut backup = file.as_os_str().to_owned();
        backup.push(".bak");
        if let Err(error) = fs::copy(file, PathBuf::from(backup)) {
            return ConnectItemResult::Failed(format!("无法备份原配置：{error}"));
        }
    }
    let text = match serde_json::to_string_pretty(&root) {
        Ok(text) => text,
        Err(error) => return ConnectItemResult::Failed(error.to_string()),
    };
    match fs::write(file, text) {
        Ok(()) => ConnectItemResult::Connected(file.display().to_string()),
        Err(error) => ConnectItemResult::Failed(format!("无法写入配置文件：{error}")),
    }
}

// ---------- skills ----------

/// True for symlinks and, on Windows, junctions (any reparse point).
pub fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copy the five bundled skills into `dest_root/<name>`; only those five directories are touched.
/// Links (dev machines point these at the repo) are left alone.
pub fn copy_skills(source_root: &Path, dest_root: &Path) -> Vec<ConnectItem> {
    SKILL_NAMES
        .iter()
        .map(|name| {
            let id = format!("skill:{name}");
            let source = source_root.join(name);
            let dest = dest_root.join(name);
            if !source.is_dir() {
                return ConnectItem::new(&id, "failed", "安装包里缺少该 skill");
            }
            if is_link(&dest) {
                return ConnectItem::new(&id, "skipped", "已链接开发目录");
            }
            let result = fs::create_dir_all(dest_root)
                .and_then(|()| {
                    if dest.exists() {
                        fs::remove_dir_all(&dest)
                    } else {
                        Ok(())
                    }
                })
                .and_then(|()| copy_dir(&source, &dest));
            match result {
                Ok(()) => ConnectItem::new(&id, "connected", dest.display().to_string()),
                Err(error) => ConnectItem::new(&id, "failed", error.to_string()),
            }
        })
        .collect()
}

// ---------- Claude Code ----------

fn run_claude(claude: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let mut command = Command::new(claude);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.output().map_err(|error| error.to_string())
}

fn connect_claude_code(exe: &Path) -> ConnectItem {
    let Some(claude) = find_claude() else {
        return ConnectItem::new(
            "claude-code",
            "skipped",
            "PATH 里没有找到 claude 命令（未安装 Claude Code）",
        );
    };
    // Re-registering fails if the name exists, so remove first; a missing entry is fine.
    let _ = run_claude(&claude, &["mcp", "remove", "--scope", "user", SERVER_NAME]);
    let exe_text = exe.to_string_lossy().into_owned();
    match run_claude(
        &claude,
        &["mcp", "add", "--scope", "user", SERVER_NAME, "--", &exe_text],
    ) {
        Ok(output) if output.status.success() => {
            ConnectItem::new("claude-code", "connected", "已执行 claude mcp add")
        }
        Ok(output) => ConnectItem::new(
            "claude-code",
            "failed",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ),
        Err(error) => ConnectItem::new("claude-code", "failed", error),
    }
}

// ---------- orchestration ----------

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(STATE_FILE))
}

fn read_state(app: &AppHandle) -> ConnectState {
    state_path(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_state(app: &AppHandle) {
    if let Some(path) = state_path(app) {
        let state = ConnectState {
            connected_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&state) {
            let _ = fs::write(path, text);
        }
    }
}

fn connect_all(app: &AppHandle) -> Vec<ConnectItem> {
    let mut items = Vec::new();
    let Some(exe) = locate_sidecar() else {
        items.push(ConnectItem::new(
            "sidecar",
            "failed",
            "找不到随安装包附带的 secrethub-mcp 程序",
        ));
        return items;
    };
    items.push(connect_claude_code(&exe));
    let claude_desktop =
        dirs::config_dir().map(|dir| dir.join("Claude").join("claude_desktop_config.json"));
    items.push(match claude_desktop {
        Some(path) => merge_mcp_json(&path, &exe).into_item("claude-desktop"),
        None => ConnectItem::new("claude-desktop", "skipped", "找不到用户配置目录"),
    });
    let cursor = dirs::home_dir().map(|dir| dir.join(".cursor").join("mcp.json"));
    items.push(match cursor {
        Some(path) => merge_mcp_json(&path, &exe).into_item("cursor"),
        None => ConnectItem::new("cursor", "skipped", "找不到用户目录"),
    });
    let skills_dest = dirs::home_dir().map(|dir| dir.join(".claude").join("skills"));
    match (locate_skills(app), skills_dest) {
        (Some(source), Some(dest)) => items.extend(copy_skills(&source, &dest)),
        _ => items.push(ConnectItem::new(
            "skills",
            "failed",
            "找不到打包的 skills 或用户目录",
        )),
    }
    write_state(app);
    items
}

#[tauri::command]
pub async fn ai_connect(app: AppHandle) -> Result<Vec<ConnectItem>, String> {
    tauri::async_runtime::spawn_blocking(move || connect_all(&app))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn ai_connect_status(app: AppHandle) -> ConnectStatus {
    ConnectStatus {
        connected_version: read_state(&app).connected_version,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        sidecar_found: locate_sidecar().is_some(),
    }
}

/// After an app upgrade, refresh skills and MCP paths if the user connected before.
pub fn refresh_after_upgrade(app: AppHandle) {
    let Some(previous) = read_state(&app).connected_version else {
        return;
    };
    if previous == env!("CARGO_PKG_VERSION") {
        return;
    }
    std::thread::spawn(move || {
        let _ = connect_all(&app);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Program Files\SecretHub\secrethub-mcp.exe")
    }

    #[test]
    fn merge_skips_when_client_directory_is_missing() {
        let dir = tempdir().unwrap();
        let result = merge_mcp_json(&dir.path().join("Claude").join("c.json"), &exe());
        assert!(matches!(result, ConnectItemResult::Skipped(_)));
        assert!(!dir.path().join("Claude").exists());
    }

    #[test]
    fn merge_creates_file_in_existing_directory() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("mcp.json");
        assert!(matches!(
            merge_mcp_json(&file, &exe()),
            ConnectItemResult::Connected(_)
        ));
        let value: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            value["mcpServers"]["secrethub"]["command"],
            exe().to_string_lossy().as_ref()
        );
        assert_eq!(value["mcpServers"]["secrethub"]["args"], json!([]));
        assert!(!dir.path().join("mcp.json.bak").exists());
    }

    #[test]
    fn merge_keeps_other_keys_and_writes_backup() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("claude_desktop_config.json");
        let original = r#"{"theme":"dark","mcpServers":{"other":{"command":"x","args":["1"]},"secrethub":{"command":"old"}}}"#;
        fs::write(&file, original).unwrap();
        assert!(matches!(
            merge_mcp_json(&file, &exe()),
            ConnectItemResult::Connected(_)
        ));
        let value: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "x");
        assert_eq!(
            value["mcpServers"]["secrethub"]["command"],
            exe().to_string_lossy().as_ref()
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("claude_desktop_config.json.bak")).unwrap(),
            original
        );
    }

    #[test]
    fn merge_never_overwrites_invalid_json() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("mcp.json");
        fs::write(&file, "{ not json").unwrap();
        assert!(matches!(
            merge_mcp_json(&file, &exe()),
            ConnectItemResult::Failed(_)
        ));
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
        assert!(!dir.path().join("mcp.json.bak").exists());
    }

    #[test]
    fn merge_rejects_non_object_shapes() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("mcp.json");
        fs::write(&file, r#"{"mcpServers":[]}"#).unwrap();
        assert!(matches!(
            merge_mcp_json(&file, &exe()),
            ConnectItemResult::Failed(_)
        ));
        fs::write(&file, "[]").unwrap();
        assert!(matches!(
            merge_mcp_json(&file, &exe()),
            ConnectItemResult::Failed(_)
        ));
    }

    fn make_source(root: &Path) {
        for name in SKILL_NAMES {
            fs::create_dir_all(root.join(name).join("references")).unwrap();
            fs::write(root.join(name).join("SKILL.md"), format!("new {name}")).unwrap();
            fs::write(root.join(name).join("references").join("a.md"), "ref").unwrap();
        }
    }

    #[test]
    fn skills_are_copied_and_only_the_five_dirs_are_replaced() {
        let dir = tempdir().unwrap();
        let (source, dest) = (dir.path().join("src"), dir.path().join("dest"));
        make_source(&source);
        fs::create_dir_all(dest.join("secrethub-import")).unwrap();
        fs::write(dest.join("secrethub-import").join("stale.md"), "old").unwrap();
        fs::create_dir_all(dest.join("unrelated")).unwrap();
        fs::write(dest.join("unrelated").join("keep.md"), "keep").unwrap();
        let items = copy_skills(&source, &dest);
        assert_eq!(items.len(), 5);
        assert!(items.iter().all(|item| item.status == "connected"));
        assert!(!dest.join("secrethub-import").join("stale.md").exists());
        assert_eq!(
            fs::read_to_string(dest.join("secrethub-import").join("SKILL.md")).unwrap(),
            "new secrethub-import"
        );
        assert!(dest
            .join("secrethub-import")
            .join("references")
            .join("a.md")
            .exists());
        assert!(dest.join("unrelated").join("keep.md").exists());
    }

    #[test]
    fn missing_source_skill_is_reported_failed() {
        let dir = tempdir().unwrap();
        let items = copy_skills(&dir.path().join("none"), &dir.path().join("dest"));
        assert!(items.iter().all(|item| item.status == "failed"));
    }

    #[cfg(windows)]
    #[test]
    fn junction_targets_are_skipped_and_not_deleted() {
        let dir = tempdir().unwrap();
        let (source, dest, dev) = (
            dir.path().join("src"),
            dir.path().join("dest"),
            dir.path().join("dev"),
        );
        make_source(&source);
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("SKILL.md"), "dev copy").unwrap();
        let link = dest.join("secrethub-import");
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&dev)
            .output()
            .unwrap();
        assert!(made.status.success(), "mklink failed");
        assert!(is_link(&link));
        let items = copy_skills(&source, &dest);
        let linked = items
            .iter()
            .find(|item| item.id == "skill:secrethub-import")
            .unwrap();
        assert_eq!(
            (linked.status.as_str(), linked.detail.as_str()),
            ("skipped", "已链接开发目录")
        );
        assert_eq!(fs::read_to_string(dev.join("SKILL.md")).unwrap(), "dev copy");
        assert!(is_link(&link));
        assert_eq!(
            items.iter().filter(|item| item.status == "connected").count(),
            4
        );
    }

    #[test]
    fn path_lookup_finds_matching_extension() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("claude.cmd"), "").unwrap();
        let path = env::join_paths([dir.path()]).unwrap();
        assert_eq!(
            find_in_path("claude", Some(path.clone()), &[".exe", ".cmd"]),
            Some(dir.path().join("claude.cmd"))
        );
        assert_eq!(find_in_path("claude", Some(path), &[".exe"]), None);
        assert_eq!(find_in_path("claude", None, &[".exe"]), None);
    }
}
