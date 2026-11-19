//! Per-OS details for finding and running executables, kept here so business modules stay
//! platform-neutral.

use std::{env, ffi::OsString};

/// File name of the MCP sidecar next to the main executable in an installed app
/// (Windows: install dir; macOS: `SecretHub.app/Contents/MacOS`).
pub const SIDECAR_FILE: &str = if cfg!(windows) {
    "secrethub-mcp.exe"
} else {
    "secrethub-mcp"
};

/// Name `npm --prefix apps/mcp run build:sea` gives the sidecar for the target being compiled.
pub fn dev_sidecar_file() -> String {
    format!(
        "secrethub-mcp-{}{}",
        env!("SECRETHUB_TARGET_TRIPLE"),
        if cfg!(windows) { ".exe" } else { "" }
    )
}

/// Whether the updater's install step relaunches the app by itself. Windows runs the NSIS installer,
/// which restarts SecretHub; macOS only swaps the .app bundle in place, so the app restarts itself.
pub const INSTALLER_RESTARTS_APP: bool = cfg!(windows);

/// Suffixes tried when looking a command up on PATH.
pub const EXECUTABLE_EXTS: &[&str] = if cfg!(windows) {
    &[".exe", ".cmd", ".bat"]
} else {
    &[""]
};

/// PATH used to find and run the user's CLI tools (e.g. `claude`).
#[cfg(not(target_os = "macos"))]
pub fn tool_search_path() -> Option<OsString> {
    env::var_os("PATH")
}

/// Apps opened from Finder or the Dock only inherit `/usr/bin:/bin:/usr/sbin:/sbin`, so the usual
/// install locations of Claude Code, Homebrew and npm are appended.
#[cfg(target_os = "macos")]
pub fn tool_search_path() -> Option<OsString> {
    use std::path::PathBuf;
    let current = env::var_os("PATH");
    let mut dirs: Vec<PathBuf> = current
        .as_ref()
        .map(|path| env::split_paths(path).collect())
        .unwrap_or_default();
    let mut extras: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        extras.push(home.join(".local/bin"));
        extras.push(home.join(".claude/local"));
        extras.push(home.join(".npm-global/bin"));
    }
    extras.push(PathBuf::from("/opt/homebrew/bin"));
    extras.push(PathBuf::from("/usr/local/bin"));
    for extra in extras {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    env::join_paths(dirs).ok().or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_sidecar_matches_build_sea_naming() {
        let name = dev_sidecar_file();
        assert!(name.starts_with("secrethub-mcp-"));
        assert!(name.contains(env!("SECRETHUB_TARGET_TRIPLE")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_tool_path_includes_homebrew_and_local_bin() {
        let path = tool_search_path().unwrap();
        let dirs: Vec<_> = env::split_paths(&path).collect();
        assert!(dirs.iter().any(|dir| dir.ends_with("opt/homebrew/bin")));
        assert!(dirs.iter().any(|dir| dir.ends_with(".local/bin")));
    }
}
