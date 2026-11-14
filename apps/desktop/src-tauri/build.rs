fn main() {
    // `tauri build` requires the sidecar to exist. `npm --prefix apps/mcp run build:sea` produces the real one;
    // a placeholder keeps cargo check/test/clippy working on a fresh checkout (never used for release builds).
    let target = std::env::var("TARGET").unwrap_or_default();
    let sidecar = std::path::Path::new("binaries").join(format!(
        "secrethub-mcp-{target}{}",
        if target.contains("windows") { ".exe" } else { "" }
    ));
    if !sidecar.exists() {
        let _ = std::fs::create_dir_all("binaries");
        let _ = std::fs::write(&sidecar, b"placeholder: run npm --prefix apps/mcp run build:sea");
    }
    tauri_build::build();
}
