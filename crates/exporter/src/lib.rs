//! Safe dotenv preview and atomic export.

use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    pub key: String,
    pub value: String,
}

impl EnvEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("invalid environment variable key: {0}")]
    InvalidKey(String),
    #[error("dotenv parse failed: {0}")]
    Parse(#[from] dotenvy::Error),
    #[error("invalid import JSON: {0}")]
    InvalidJson(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn render_env(entries: &[EnvEntry]) -> Result<String, ExportError> {
    let mut output = String::new();
    for entry in entries {
        validate_key(&entry.key)?;
        output.push_str(&entry.key);
        output.push_str("=\"");
        output.push_str(&escape_value(&entry.value));
        output.push_str("\"\n");
    }
    Ok(output)
}

pub fn render_example(entries: &[EnvEntry]) -> Result<String, ExportError> {
    render_env(
        &entries
            .iter()
            .map(|entry| EnvEntry::new(&entry.key, ""))
            .collect::<Vec<_>>(),
    )
}

pub fn parse_env_entries(contents: &str) -> Result<Vec<EnvEntry>, ExportError> {
    dotenvy::from_read_iter(Cursor::new(contents.as_bytes()))
        .map(|item| {
            let (key, value) = item?;
            validate_key(&key)?;
            Ok(EnvEntry::new(key, value))
        })
        .collect()
}

pub fn parse_json_entries(contents: &str) -> Result<Vec<EnvEntry>, ExportError> {
    let value: serde_json::Value = serde_json::from_str(contents)
        .map_err(|error| ExportError::InvalidJson(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ExportError::InvalidJson("import JSON must be an object".to_owned()))?;
    object
        .iter()
        .map(|(key, value)| {
            validate_key(key)?;
            let value = value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| value.as_number().map(ToString::to_string))
                .or_else(|| value.as_bool().map(|value| value.to_string()))
                .ok_or_else(|| {
                    ExportError::InvalidJson(format!(
                        "JSON value for {key} must be a string, number or boolean"
                    ))
                })?;
            Ok(EnvEntry::new(key, value))
        })
        .collect()
}

pub fn detect_conflicts(existing: &str, desired: &[EnvEntry]) -> Result<Vec<String>, ExportError> {
    let existing_keys: HashSet<String> = dotenvy::from_read_iter(Cursor::new(existing.as_bytes()))
        .map(|item| item.map(|(key, _)| key))
        .collect::<Result<_, _>>()?;
    Ok(desired
        .iter()
        .filter(|entry| existing_keys.contains(&entry.key))
        .map(|entry| entry.key.clone())
        .collect())
}

pub fn validate_export_directory(path: &Path) -> Result<PathBuf, ExportError> {
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "export target is not a directory",
        )
        .into());
    }
    Ok(canonical)
}

pub fn write_atomic(
    path: &Path,
    contents: &str,
    replace_existing: bool,
) -> Result<(), ExportError> {
    if path.exists() && !replace_existing {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "target exists; explicit replacement required",
        )
        .into());
    }
    let temp_path = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{}.", value))
            .unwrap_or_default()
    ));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    if replace_existing && path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp_path, path)?;
    Ok(())
}

pub fn gitignore_has_env(path: &Path) -> Result<bool, ExportError> {
    if !path.exists() {
        return Ok(false);
    }
    let contents = fs::read_to_string(path)?;
    Ok(contents
        .lines()
        .map(str::trim)
        .any(|line| matches!(line, ".env" | ".env.*" | "*.env" | "**/.env")))
}

pub fn ensure_gitignore_env(path: &Path) -> Result<bool, ExportError> {
    if gitignore_has_env(path)? {
        return Ok(false);
    }
    let previous = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let separator = if previous.is_empty() || previous.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    write_atomic(path, &format!("{}{}.env\n", previous, separator), true)?;
    Ok(true)
}

pub fn env_is_tracked(directory: &Path) -> Result<bool, ExportError> {
    let status = std::process::Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", ".env"])
        .current_dir(directory)
        .output()?;
    Ok(status.status.success())
}

fn validate_key(key: &str) -> Result<(), ExportError> {
    let mut chars = key.chars();
    let valid = matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(ExportError::InvalidKey(key.to_owned()))
    }
}

fn escape_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
