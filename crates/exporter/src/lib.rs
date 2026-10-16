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
    pub comment: Option<String>,
    pub separator_before: bool,
}

impl EnvEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            comment: None,
            separator_before: false,
        }
    }

    pub fn with_comment(
        key: impl Into<String>,
        value: impl Into<String>,
        comment: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            comment: Some(comment.into()),
            separator_before: false,
        }
    }

    pub fn with_separator_before(mut self) -> Self {
        self.separator_before = true;
        self
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
        if entry.separator_before && !output.is_empty() && !output.ends_with("\n\n") {
            output.push('\n');
        }
        if let Some(comment) = &entry.comment {
            for line in comment.lines() {
                output.push_str("# ");
                output.push_str(line.trim());
                output.push('\n');
            }
        }
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
            .map(|entry| {
                let mut example = EnvEntry::new(&entry.key, "");
                example.comment = entry.comment.clone();
                example.separator_before = entry.separator_before;
                example
            })
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

/// Upserts `entries` into an existing dotenv file. Unrelated lines, comments and
/// ordering are kept; planned keys are replaced in place and new keys appended.
pub fn merge_env(existing: &str, entries: &[EnvEntry]) -> Result<String, ExportError> {
    let (existing, crlf) = normalize_newlines(existing);
    parse_env_entries(&existing)?;
    let mut output = String::new();
    let mut written = HashSet::new();
    let mut lines = existing.lines();
    while let Some(line) = lines.next() {
        let key = assignment_key(line);
        let mut block = format!("{line}\n");
        if let Some(quote) = open_quote(line) {
            for next in lines.by_ref() {
                block.push_str(next);
                block.push('\n');
                if find_unescaped(next, quote).is_some() {
                    break;
                }
            }
        }
        match key.and_then(|key| entries.iter().find(|entry| entry.key == key)) {
            Some(entry) => {
                if written.insert(entry.key.clone()) {
                    output.push_str(&render_env(&[EnvEntry::new(&entry.key, &entry.value)])?);
                }
            }
            None => output.push_str(&block),
        }
    }
    let missing = entries
        .iter()
        .filter(|entry| !written.contains(&entry.key))
        .cloned()
        .collect::<Vec<_>>();
    append_rendered(&mut output, &render_env(&missing)?);
    Ok(restore_newlines(output, crlf))
}

/// Appends empty example lines for keys the existing `.env.example` lacks.
pub fn merge_example(existing: &str, entries: &[EnvEntry]) -> Result<String, ExportError> {
    let (existing, crlf) = normalize_newlines(existing);
    let present = parse_env_entries(&existing)?
        .into_iter()
        .map(|entry| entry.key)
        .collect::<HashSet<_>>();
    let missing = entries
        .iter()
        .filter(|entry| !present.contains(&entry.key))
        .cloned()
        .collect::<Vec<_>>();
    let mut output = existing;
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    append_rendered(&mut output, &render_example(&missing)?);
    Ok(restore_newlines(output, crlf))
}

fn normalize_newlines(text: &str) -> (String, bool) {
    (text.replace("\r\n", "\n"), text.contains("\r\n"))
}

fn restore_newlines(text: String, crlf: bool) -> String {
    if crlf {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

fn append_rendered(output: &mut String, rendered: &str) {
    if rendered.is_empty() {
        return;
    }
    if !output.is_empty() && !output.ends_with("\n\n") {
        output.push('\n');
    }
    output.push_str(rendered);
}

fn assignment_key(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    validate_key(key).ok().map(|_| key)
}

/// Returns the quote character when the line's value opens a quoted string
/// that continues on following lines.
fn open_quote(line: &str) -> Option<char> {
    assignment_key(line)?;
    let (_, value) = line.split_once('=')?;
    let value = value.trim_start();
    let quote = value.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
    find_unescaped(&value[1..], quote)
        .is_none()
        .then_some(quote)
}

fn find_unescaped(text: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quote == '"' {
            escaped = true;
        } else if ch == quote {
            return Some(index);
        }
    }
    None
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
