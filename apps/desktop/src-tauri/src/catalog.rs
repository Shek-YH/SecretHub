//! Effective model catalog: bundled registry plus an optional verified remote overlay.
//! Holds the in-process state and exports the models-only view for the frontend and MCP.

use secrethub_providers::{
    default_registry, effective_catalog, effective_models, EffectiveCatalog, EffectiveModels,
    Overlay, ProviderDefinition,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, RwLock},
};

pub const EFFECTIVE_MODELS_FILE: &str = "effective-models.json";

pub struct CatalogState {
    current: RwLock<Arc<EffectiveCatalog>>,
}

impl CatalogState {
    pub fn new() -> Self {
        Self {
            current: RwLock::new(Arc::new(effective_catalog(&default_registry(), None))),
        }
    }

    pub fn current(&self) -> Arc<EffectiveCatalog> {
        self.current
            .read()
            .map(|guard| Arc::clone(&guard))
            .unwrap_or_else(|poisoned| Arc::clone(&poisoned.into_inner()))
    }

    /// Replaces the effective catalog; returns whether the models view changed.
    #[allow(dead_code)] // called by the remote-update path (C3)
    pub fn apply(&self, overlay: Option<&Overlay>) -> bool {
        let next = Arc::new(effective_catalog(&default_registry(), overlay));
        let previous = self.current();
        let changed = effective_models(&previous) != effective_models(&next);
        match self.current.write() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
        changed
    }
}

static STATE: LazyLock<CatalogState> = LazyLock::new(CatalogState::new);

#[allow(dead_code)] // used by the remote-update path (C3)
pub fn state() -> &'static CatalogState {
    &STATE
}

/// Registry used for capability inference: bundled data with the effective model lists.
pub fn effective_registry() -> Vec<ProviderDefinition> {
    STATE.current().providers.clone()
}

pub fn current_models() -> EffectiveModels {
    effective_models(&STATE.current())
}

pub fn models_path(data_dir: &Path) -> PathBuf {
    data_dir.join("catalog").join(EFFECTIVE_MODELS_FILE)
}

/// Writes the models-only view for the MCP server; skips the write when unchanged.
/// Returns whether the file was (re)written.
pub fn write_effective_models(path: &Path, models: &EffectiveModels) -> io::Result<bool> {
    let mut bytes = serde_json::to_vec(models).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if fs::read(path)
        .map(|existing| existing == bytes)
        .unwrap_or(false)
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, &bytes)?;
    fs::rename(&temp, path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_providers::{bundled_serial, parse_overlay, CatalogOrigin};

    fn overlay(serial: u64, model: &str) -> Overlay {
        let json = format!(
            r#"{{"schema":1,"serial":{serial},"publishedAt":"2026-10-17T00:00:00Z","providers":{{"deepseek":{{"models":[{{"id":"{model}"}}]}}}}}}"#
        );
        parse_overlay(json.as_bytes()).unwrap()
    }

    #[test]
    fn apply_reports_changes_and_switches_origin() {
        let state = CatalogState::new();
        assert_eq!(state.current().origin, CatalogOrigin::Bundled);
        assert_eq!(state.current().serial, bundled_serial().serial);
        assert!(state.apply(Some(&overlay(2026101700, "ds-a"))));
        assert_eq!(state.current().origin, CatalogOrigin::Remote);
        assert!(!state.apply(Some(&overlay(2026101700, "ds-a"))));
        assert!(state.apply(Some(&overlay(2026101701, "ds-a"))));
        assert!(state.apply(None));
        assert_eq!(state.current().origin, CatalogOrigin::Bundled);
    }

    #[test]
    fn writes_effective_models_only_when_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = models_path(dir.path());
        let state = CatalogState::new();
        let bundled = effective_models(&state.current());
        assert!(write_effective_models(&path, &bundled).unwrap());
        assert!(!write_effective_models(&path, &bundled).unwrap());
        state.apply(Some(&overlay(2026101700, "ds-a")));
        let remote = effective_models(&state.current());
        assert!(write_effective_models(&path, &remote).unwrap());
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["serial"], 2026101700);
        assert_eq!(written["providers"]["deepseek"]["models"][0]["id"], "ds-a");
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!String::from_utf8(fs::read(&path).unwrap())
            .unwrap()
            .contains("http"));
    }
}
