//! Local import scanning (design section 3.2). Reads files and folders the user pointed at and
//! turns them into candidate config groups. Everything here runs inside the Desktop process:
//! values are carried in memory but never logged, returned to the web view or put in errors.

use secrethub_core::groups::{is_secret_name, valid_variable_name, MAX_VALUE_BYTES, MAX_VARIABLES};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MAX_DEPTH: usize = 3;
pub const MAX_FILES: usize = 200;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_NOTES_CHARS: usize = 100_000;
const KEY_NAMES: [&str; 4] = ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"];
const TEXT_EXTENSIONS: [&str; 4] = ["txt", "csv", "log", "md"];

/// One variable found in the files of a group. `values` holds every distinct value seen under
/// this name in file order; the review lets the user pick one (default: the last).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanVar {
    pub name: String,
    pub values: Vec<String>,
    pub secret: bool,
    /// The private key was added next to a different one; the review marks it.
    pub conflict: bool,
}

impl ScanVar {
    pub fn duplicate(&self) -> bool {
        self.values.len() > 1
    }

    pub fn last_index(&self) -> usize {
        self.values.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedGroup {
    /// Folder (or file) the group came from, for display and grouping in the review.
    pub source: String,
    pub name: String,
    pub variables: Vec<ScanVar>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub name: String,
    pub reason: &'static str,
}

/// A candidate that is not a config group: a login, bank card or secure note. `fields` use
/// the keys of the typed item definitions (`item-types.json`); a note carries its text in `body`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extra {
    pub source: String,
    /// `login` | `bank_card` | `note`
    pub kind: &'static str,
    pub name: String,
    pub fields: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub groups: Vec<ScannedGroup>,
    pub extras: Vec<Extra>,
    pub skipped: Vec<SkippedFile>,
}

/// What one folder (or single file) yields.
pub struct GroupScan {
    pub group: Option<ScannedGroup>,
    pub extras: Vec<Extra>,
    pub skipped: Vec<SkippedFile>,
}

/// A file read into memory.
#[derive(Debug, Clone)]
pub struct FileInput {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Env,
    Json,
    PrivateKey,
    PublicKey,
    Text,
    Unsupported,
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub(crate) fn classify(file: &FileInput) -> Kind {
    let lower = file.name.to_ascii_lowercase();
    let ext = extension(&file.name);
    if ext == "pub" {
        return Kind::PublicKey;
    }
    let head = file.text.trim_start();
    if head.starts_with("-----BEGIN") && head.contains("PRIVATE KEY") {
        return Kind::PrivateKey;
    }
    if KEY_NAMES.contains(&lower.as_str()) || ext == "pem" {
        return Kind::PrivateKey;
    }
    if lower.starts_with(".env") || lower.ends_with(".env") || ext == "env" {
        return Kind::Env;
    }
    if ext == "json" {
        return Kind::Json;
    }
    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return Kind::Text;
    }
    Kind::Unsupported
}

fn order(kind: Kind) -> u8 {
    match kind {
        Kind::Env | Kind::Json => 0,
        Kind::PrivateKey => 1,
        Kind::PublicKey => 2,
        Kind::Text => 3,
        Kind::Unsupported => 4,
    }
}

// ----------------------------------------------------------------------- .env parsing

/// Entries in file order (duplicates kept) and the meaningful lines that are not `KEY=VALUE`.
pub struct EnvParse {
    pub entries: Vec<(String, String)>,
    pub other_lines: Vec<String>,
}

/// dotenvy first (it understands quoting precisely); when it fails, or the file contains `$`
/// (dotenvy would expand it inside passwords), a line parser takes over.
pub fn parse_env(text: &str) -> EnvParse {
    if !text.contains('$') {
        if let Ok(entries) = secrethub_exporter::parse_env_entries(text) {
            return EnvParse {
                entries: entries
                    .into_iter()
                    .map(|entry| (entry.key, entry.value))
                    .collect(),
                other_lines: Vec::new(),
            };
        }
    }
    parse_env_lines(text)
}

fn unescape_double(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(ch) = characters.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Position of the closing quote in `rest` (no escapes for single quotes).
fn closing_quote(rest: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in rest.char_indices() {
        if quote == '"' && escaped {
            escaped = false;
            continue;
        }
        if quote == '"' && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(index);
        }
    }
    None
}

/// One `KEY=VALUE` assignment starting at `lines[index]`: the name, the value and the index of
/// the next line (a quoted value may span several lines). `None` when the line is not one.
pub(crate) fn parse_env_assignment(
    lines: &[&str],
    index: usize,
) -> Option<(String, String, usize)> {
    let line = lines[index].trim();
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (name, rest) = line.split_once('=')?;
    let name = name.trim();
    if !valid_variable_name(name) {
        return None;
    }
    let mut next = index + 1;
    let rest = rest.trim_start();
    let value = match rest.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let body = &rest[1..];
            if let Some(end) = closing_quote(body, quote) {
                let inner = &body[..end];
                if quote == '"' {
                    unescape_double(inner)
                } else {
                    inner.to_owned()
                }
            } else {
                // Multi-line quoted value: read on until a line closes the quote.
                let mut collected = vec![body.to_owned()];
                let mut closed = None;
                let mut cursor = next;
                while cursor < lines.len() {
                    let following = lines[cursor];
                    cursor += 1;
                    if let Some(end) = closing_quote(following, quote) {
                        collected.push(following[..end].to_owned());
                        closed = Some(cursor);
                        break;
                    }
                    collected.push(following.to_owned());
                }
                match closed {
                    Some(after) => {
                        next = after;
                        let joined = collected.join("\n");
                        if quote == '"' {
                            unescape_double(&joined)
                        } else {
                            joined
                        }
                    }
                    // Never closed: keep what the first line says.
                    None => body.to_owned(),
                }
            }
        }
        _ => {
            let unquoted = rest.split(" #").next().unwrap_or(rest);
            unquoted.trim_end().to_owned()
        }
    };
    Some((name.to_owned(), value, next))
}

fn parse_env_lines(text: &str) -> EnvParse {
    let lines: Vec<&str> = text.lines().collect();
    let mut entries = Vec::new();
    let mut other_lines = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() || line.starts_with('#') {
            index += 1;
            continue;
        }
        match parse_env_assignment(&lines, index) {
            Some((name, value, next)) => {
                entries.push((name, value));
                index = next;
            }
            None => {
                other_lines.push(line.to_owned());
                index += 1;
            }
        }
    }
    EnvParse {
        entries,
        other_lines,
    }
}

/// A group built from loose `(name, value)` pairs (smart paste); `None` when nothing is usable.
pub(crate) fn group_from_entries(
    source: &str,
    name: &str,
    entries: Vec<(String, String)>,
) -> (Option<ScannedGroup>, Vec<SkippedFile>) {
    let mut builder = Builder {
        variables: Vec::new(),
        notes: Vec::new(),
        skipped: Vec::new(),
        extras: Vec::new(),
    };
    for (key, value) in entries {
        builder.add(&key, value);
    }
    let group = (!builder.variables.is_empty()).then(|| ScannedGroup {
        source: source.to_owned(),
        name: name.to_owned(),
        variables: builder.variables,
        notes: String::new(),
    });
    (group, builder.skipped)
}

// ----------------------------------------------------------------------- group assembly

struct Builder {
    variables: Vec<ScanVar>,
    notes: Vec<String>,
    skipped: Vec<SkippedFile>,
    extras: Vec<Extra>,
}

impl Builder {
    fn add(&mut self, name: &str, value: String) {
        if value.trim().is_empty() {
            return;
        }
        if value.len() > MAX_VALUE_BYTES {
            self.skipped.push(SkippedFile {
                name: name.to_owned(),
                reason: "value_too_large",
            });
            return;
        }
        match self.variables.iter_mut().find(|var| var.name == name) {
            Some(existing) => {
                if !existing.values.contains(&value) {
                    existing.values.push(value);
                }
            }
            None => {
                if self.variables.len() >= MAX_VARIABLES {
                    self.skipped.push(SkippedFile {
                        name: name.to_owned(),
                        reason: "too_many_variables",
                    });
                    return;
                }
                self.variables.push(ScanVar {
                    name: name.to_owned(),
                    values: vec![value],
                    secret: is_secret_name(name),
                    conflict: false,
                });
            }
        }
    }

    fn note(&mut self, file: &str, body: &str) {
        let body = body.trim_end();
        if !body.trim().is_empty() {
            self.notes.push(format!("来源文件：{file}\n{body}"));
        }
    }

    fn holds_value(&self, value: &str) -> bool {
        self.variables.iter().any(|var| {
            var.values
                .iter()
                .any(|existing| existing.trim() == value.trim())
        })
    }

    fn free_name(&self, base: &str) -> (String, bool) {
        if !self.variables.iter().any(|var| var.name == base) {
            return (base.to_owned(), false);
        }
        let mut counter = 2;
        loop {
            let candidate = format!("{base}_{counter}");
            if !self.variables.iter().any(|var| var.name == candidate) {
                return (candidate, true);
            }
            counter += 1;
        }
    }
}

pub(crate) fn normalized_key(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_owned() + "\n"
}

/// Flat JSON object of scalars -> variables; anything else is `None` (the caller makes it a note).
pub(crate) fn json_variables(text: &str) -> Option<Vec<(String, String)>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let mut out = Vec::new();
    for (key, value) in object {
        if !valid_variable_name(key) {
            return None;
        }
        let text = match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            serde_json::Value::Null => String::new(),
            _ => return None,
        };
        out.push((key.clone(), text));
    }
    Some(out)
}

/// Builds the candidate group of one folder (or single file) from its files. `None` when
/// nothing usable was found.
#[cfg(test)]
pub fn assemble_group(
    source: &str,
    name: &str,
    files: &[FileInput],
) -> (Option<ScannedGroup>, Vec<SkippedFile>) {
    let scanned = assemble(source, name, files);
    (scanned.group, scanned.skipped)
}

/// Like `assemble_group`, plus the logins and bank cards found in CSV files.
pub fn assemble(source: &str, name: &str, files: &[FileInput]) -> GroupScan {
    let mut builder = Builder {
        variables: Vec::new(),
        notes: Vec::new(),
        skipped: Vec::new(),
        extras: Vec::new(),
    };
    let mut sorted: Vec<(Kind, &FileInput)> =
        files.iter().map(|file| (classify(file), file)).collect();
    sorted.sort_by(|left, right| {
        order(left.0)
            .cmp(&order(right.0))
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    for (kind, file) in sorted {
        if file.text.trim().is_empty() {
            builder.skipped.push(SkippedFile {
                name: file.name.clone(),
                reason: "empty",
            });
            continue;
        }
        match kind {
            Kind::Env => {
                let parsed = parse_env(&file.text);
                for (key, value) in parsed.entries {
                    builder.add(&key, value);
                }
                builder.note(&file.name, &parsed.other_lines.join("\n"));
            }
            Kind::Json => match json_variables(&file.text) {
                Some(entries) if !entries.is_empty() => {
                    for (key, value) in entries {
                        builder.add(&key, value);
                    }
                }
                _ => builder.note(&file.name, &file.text),
            },
            Kind::PrivateKey => {
                let key = normalized_key(&file.text);
                if builder.holds_value(&key) {
                    continue;
                }
                let (variable, conflict) = builder.free_name("SSH_PRIVATE_KEY");
                builder.add(&variable, key);
                if conflict {
                    if let Some(added) = builder.variables.iter_mut().find(|v| v.name == variable) {
                        added.conflict = true;
                    }
                }
            }
            Kind::PublicKey => {
                // The same key already present (for example in the .env) is not added again.
                if builder.holds_value(&file.text) {
                    continue;
                }
                let (variable, _) = builder.free_name("SSH_PUBLIC_KEY");
                builder.add(&variable, file.text.trim().to_owned());
            }
            Kind::Text => {
                // Password exports and the bank card template become items of their own.
                if extension(&file.name) == "csv" {
                    if let Some(csv) =
                        crate::csv_import::extras_from_csv(&file.name, &file.text, source)
                    {
                        builder.extras.extend(csv.extras);
                        builder.skipped.extend(csv.skipped);
                        continue;
                    }
                }
                builder.note(&file.name, &file.text)
            }
            Kind::Unsupported => builder.skipped.push(SkippedFile {
                name: file.name.clone(),
                reason: "unsupported",
            }),
        }
    }
    let mut notes = builder.notes.join("\n\n");
    if notes.chars().count() > MAX_NOTES_CHARS {
        notes = notes.chars().take(MAX_NOTES_CHARS).collect();
        builder.skipped.push(SkippedFile {
            name: name.to_owned(),
            reason: "notes_truncated",
        });
    }
    let group = (!(builder.variables.is_empty() && notes.is_empty())).then(|| ScannedGroup {
        source: source.to_owned(),
        name: name.to_owned(),
        variables: builder.variables,
        notes,
    });
    GroupScan {
        group,
        extras: builder.extras,
        skipped: builder.skipped,
    }
}

// ------------------------------------------------------------------- filesystem walking

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("folder")
        .to_owned()
}

/// Reads one file for scanning, or says why it is not usable (names only, never content).
fn read_candidate(path: &Path) -> Result<FileInput, SkippedFile> {
    let name = display_name(path);
    let skipped = |reason| SkippedFile {
        name: name.clone(),
        reason,
    };
    let metadata = fs::metadata(path).map_err(|_| skipped("unreadable"))?;
    if !metadata.is_file() {
        return Err(skipped("not_a_file"));
    }
    if metadata.len() == 0 {
        return Err(skipped("empty"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(skipped("too_large"));
    }
    let bytes = fs::read(path).map_err(|_| skipped("unreadable"))?;
    if bytes.contains(&0) {
        return Err(skipped("binary"));
    }
    let text = String::from_utf8(bytes).map_err(|_| skipped("binary"))?;
    Ok(FileInput { name, text })
}

fn walk(
    directory: &Path,
    depth: usize,
    budget: &mut usize,
    result: &mut ScanResult,
) -> Result<(), String> {
    let mut files = Vec::new();
    let mut subdirectories = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|_| "cannot read folder".to_owned())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            let name = display_name(&path);
            if depth < MAX_DEPTH && name != ".git" && name != "node_modules" {
                subdirectories.push(path);
            }
            continue;
        }
        if *budget == 0 {
            result.skipped.push(SkippedFile {
                name: display_name(&path),
                reason: "file_limit",
            });
            continue;
        }
        *budget -= 1;
        match read_candidate(&path) {
            Ok(file) => files.push(file),
            Err(skipped) => result.skipped.push(skipped),
        }
    }
    if !files.is_empty() {
        let name = display_name(directory);
        let scanned = assemble(&name, &name, &files);
        result.skipped.extend(scanned.skipped);
        result.groups.extend(scanned.group);
        result.extras.extend(scanned.extras);
    }
    for sub in subdirectories {
        walk(&sub, depth + 1, budget, result)?;
    }
    Ok(())
}

/// Group name of a single imported file: its stem, or the folder name for `.env` files.
fn single_file_group_name(path: &Path) -> String {
    let file = display_name(path).to_ascii_lowercase();
    let envish = file.starts_with(".env") || file.ends_with(".env") || extension(&file) == "env";
    if envish {
        if let Some(parent) = path.parent().map(display_name).filter(|n| !n.is_empty()) {
            return parent;
        }
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .map_or_else(|| display_name(path), str::to_owned)
}

/// The files an AI inspection covers (design 2026-11-10 section 1): the same range limits as
/// `scan_paths` (depth, file count, size, text only). `FileInput::name` is `<folder>/<file>`.
pub struct ReadFiles {
    pub files: Vec<FileInput>,
    pub skipped: Vec<SkippedFile>,
}

fn collect(
    directory: &Path,
    depth: usize,
    budget: &mut usize,
    out: &mut ReadFiles,
) -> Result<(), String> {
    let folder = display_name(directory);
    let mut subdirectories = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|_| "cannot read folder".to_owned())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            let name = display_name(&path);
            if depth < MAX_DEPTH && name != ".git" && name != "node_modules" {
                subdirectories.push(path);
            }
            continue;
        }
        if *budget == 0 {
            out.skipped.push(SkippedFile {
                name: display_name(&path),
                reason: "file_limit",
            });
            continue;
        }
        *budget -= 1;
        match read_candidate(&path) {
            Ok(mut file) => {
                file.name = format!("{folder}/{}", file.name);
                out.files.push(file);
            }
            Err(skipped) => out.skipped.push(skipped),
        }
    }
    for sub in subdirectories {
        collect(&sub, depth + 1, budget, out)?;
    }
    Ok(())
}

/// Reads the files behind `paths` for an inspection. Paths must be absolute and exist.
pub fn read_files(paths: &[PathBuf]) -> Result<ReadFiles, String> {
    if paths.is_empty() {
        return Err("no paths to inspect".to_owned());
    }
    let mut out = ReadFiles {
        files: Vec::new(),
        skipped: Vec::new(),
    };
    let mut budget = MAX_FILES;
    for path in paths {
        if !path.is_absolute() {
            return Err("import paths must be absolute".to_owned());
        }
        if path.is_dir() {
            collect(path, 0, &mut budget, &mut out)?;
        } else if path.is_file() {
            if budget == 0 {
                out.skipped.push(SkippedFile {
                    name: display_name(path),
                    reason: "file_limit",
                });
                continue;
            }
            budget -= 1;
            match read_candidate(path) {
                Ok(mut file) => {
                    let folder = path.parent().map(display_name).unwrap_or_default();
                    if !folder.is_empty() {
                        file.name = format!("{folder}/{}", file.name);
                    }
                    out.files.push(file);
                }
                Err(skipped) => out.skipped.push(skipped),
            }
        } else {
            return Err("import path does not exist".to_owned());
        }
    }
    Ok(out)
}

/// Scans files and folders. Paths must be absolute and exist.
pub fn scan_paths(paths: &[PathBuf]) -> Result<ScanResult, String> {
    if paths.is_empty() {
        return Err("no paths to import".to_owned());
    }
    let mut result = ScanResult::default();
    let mut budget = MAX_FILES;
    for path in paths {
        if !path.is_absolute() {
            return Err("import paths must be absolute".to_owned());
        }
        if path.is_dir() {
            walk(path, 0, &mut budget, &mut result)?;
        } else if path.is_file() {
            if budget == 0 {
                result.skipped.push(SkippedFile {
                    name: display_name(path),
                    reason: "file_limit",
                });
                continue;
            }
            budget -= 1;
            match read_candidate(path) {
                Ok(file) => {
                    let name = single_file_group_name(path);
                    let scanned = assemble(&display_name(path), &name, &[file]);
                    result.skipped.extend(scanned.skipped);
                    result.groups.extend(scanned.group);
                    result.extras.extend(scanned.extras);
                }
                Err(skipped) => result.skipped.push(skipped),
            }
        } else {
            return Err("import path does not exist".to_owned());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_KEY: &str =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----";
    const OTHER_KEY: &str =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nOTHEROTHER\n-----END OPENSSH PRIVATE KEY-----";

    fn file(name: &str, text: &str) -> FileInput {
        FileInput {
            name: name.to_owned(),
            text: text.to_owned(),
        }
    }

    fn var<'a>(group: &'a ScannedGroup, name: &str) -> &'a ScanVar {
        group
            .variables
            .iter()
            .find(|var| var.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    }

    #[test]
    fn env_parsing_handles_exports_quotes_multiline_values_and_free_text() {
        let text = "# comment\nexport A=1\nB=\"two words\"\nC='single $x'\nD=plain # trailing\nE=\"line1\nline2\"\nthis is a free text line\nbad key=1\nF=\"esc\\nnl\"\n";
        let parsed = parse_env_lines(text);
        let map: Vec<(&str, &str)> = parsed
            .entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            map,
            [
                ("A", "1"),
                ("B", "two words"),
                ("C", "single $x"),
                ("D", "plain"),
                ("E", "line1\nline2"),
                ("F", "esc\nnl")
            ]
        );
        assert_eq!(
            parsed.other_lines,
            ["this is a free text line", "bad key=1"]
        );
        // A clean file goes through dotenvy; `$` keeps the line parser (no expansion).
        assert_eq!(parse_env("A=1\nB=2\n").entries.len(), 2);
        assert!(parse_env("A=1\nB=2\n").other_lines.is_empty());
        assert_eq!(parse_env("PW=pa$$word\n").entries[0].1, "pa$$word");
    }

    #[test]
    fn duplicates_in_one_env_file_keep_every_distinct_value_last_is_the_default() {
        let env = "SSH_PRIVATE_KEY=\"first\"\nHOST=h\nSSH_PRIVATE_KEY=\"second\"\nHOST=h\n";
        let (group, skipped) = assemble_group("srv", "srv", &[file(".env", env)]);
        assert!(skipped.is_empty());
        let group = group.unwrap();
        let key = var(&group, "SSH_PRIVATE_KEY");
        assert!(key.duplicate() && key.secret);
        assert_eq!(key.values, ["first", "second"]);
        assert_eq!(key.values[key.last_index()], "second");
        // An identical repeat is not a duplicate.
        assert!(!var(&group, "HOST").duplicate());
    }

    #[test]
    fn private_key_files_skip_identical_values_and_number_different_ones() {
        // Same content as a variable already in the group: skipped.
        let env = format!("SSH_PRIVATE_KEY=\"{}\"\n", FAKE_KEY.replace('\n', "\\n"));
        let (group, _) = assemble_group(
            "a",
            "a",
            &[file("id_ed25519", FAKE_KEY), file(".env", &env)],
        );
        let group = group.unwrap();
        assert_eq!(group.variables.len(), 1);
        assert!(!group.variables[0].conflict);

        // A different key next to an existing SSH_PRIVATE_KEY becomes _2 and is flagged.
        let (group, _) = assemble_group(
            "a",
            "a",
            &[file("id_ed25519", OTHER_KEY), file(".env", &env)],
        );
        let group = group.unwrap();
        assert_eq!(var(&group, "SSH_PRIVATE_KEY").values.len(), 1);
        let second = var(&group, "SSH_PRIVATE_KEY_2");
        assert!(second.conflict && second.secret);
        assert!(second.values[0].starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));

        // Alone, a key file becomes SSH_PRIVATE_KEY (detected by content, whatever its name).
        let (group, _) = assemble_group("b", "b", &[file("key_ed25519", FAKE_KEY)]);
        let key = var(group.as_ref().unwrap(), "SSH_PRIVATE_KEY");
        assert!(!key.conflict);
        // Two different keys without any variable: first plain, second numbered.
        let (group, _) = assemble_group(
            "c",
            "c",
            &[file("id_rsa", FAKE_KEY), file("id_rsa2.pem", OTHER_KEY)],
        );
        let group = group.unwrap();
        assert!(!var(&group, "SSH_PRIVATE_KEY").conflict);
        assert!(var(&group, "SSH_PRIVATE_KEY_2").conflict);
    }

    #[test]
    fn public_keys_are_plain_variables() {
        let (group, _) = assemble_group(
            "a",
            "a",
            &[file("id_ed25519.pub", "ssh-ed25519 AAAAFAKE user@host\n")],
        );
        let group = group.unwrap();
        let public = var(&group, "SSH_PUBLIC_KEY");
        assert!(!public.secret);
        assert_eq!(public.values[0], "ssh-ed25519 AAAAFAKE user@host");
    }

    #[test]
    fn a_public_key_file_with_the_value_already_in_the_env_is_not_added_again() {
        let key = "ssh-ed25519 AAAAFAKE user@host";
        let (group, _) = assemble_group(
            "a",
            "a",
            &[
                file(".env", &format!("SERVER_SSH_PUBLIC_KEY={key}\nA=1\n")),
                file("id_ed25519.pub", &format!("{key}\n")),
            ],
        );
        let group = group.unwrap();
        assert!(group.variables.iter().all(|v| v.name != "SSH_PUBLIC_KEY"));
        assert_eq!(var(&group, "SERVER_SSH_PUBLIC_KEY").values, [key]);

        // A different key is still added.
        let (group, _) = assemble_group(
            "a",
            "a",
            &[
                file(".env", &format!("SERVER_SSH_PUBLIC_KEY={key}\n")),
                file("id_ed25519.pub", "ssh-ed25519 AAAAOTHER user@host\n"),
            ],
        );
        assert_eq!(
            var(&group.unwrap(), "SSH_PUBLIC_KEY").values,
            ["ssh-ed25519 AAAAOTHER user@host"]
        );
    }

    #[test]
    fn json_text_and_empty_files_follow_the_rules() {
        let files = [
            file(
                "config.json",
                "{\"API_HOST\":\"h\",\"PORT\":8080,\"DEBUG\":true}",
            ),
            file("nested.json", "{\"a\":{\"b\":1}}"),
            file("notes.txt", "remember the firewall\n"),
            file("users.csv", "name,team\nfake,ops\n"),
            file("empty.json", ""),
            file("blank.txt", "  \n"),
            file("photo.bin", "xx"),
        ];
        let (group, skipped) = assemble_group("srv", "srv", &files);
        let group = group.unwrap();
        assert_eq!(var(&group, "API_HOST").values, ["h"]);
        assert_eq!(var(&group, "PORT").values, ["8080"]);
        assert_eq!(var(&group, "DEBUG").values, ["true"]);
        assert!(group
            .notes
            .contains("来源文件：notes.txt\nremember the firewall"));
        assert!(group.notes.contains("来源文件：users.csv\nname,team"));
        // The nested JSON object is kept whole as a note.
        assert!(group.notes.contains("来源文件：nested.json"));
        let reasons: Vec<(&str, &str)> = skipped
            .iter()
            .map(|s| (s.name.as_str(), s.reason))
            .collect();
        assert!(reasons.contains(&("empty.json", "empty")));
        assert!(reasons.contains(&("blank.txt", "empty")));
        assert!(reasons.contains(&("photo.bin", "unsupported")));
    }

    #[test]
    fn env_free_text_lines_become_notes_with_their_source() {
        let env = "just words about the server\nHOST=h\n";
        let (group, _) = assemble_group("srv", "srv", &[file(".env.production", env)]);
        let group = group.unwrap();
        assert_eq!(var(&group, "HOST").values, ["h"]);
        assert_eq!(
            group.notes,
            "来源文件：.env.production\njust words about the server"
        );
        // A file made only of text still yields a (variable-less) group; an empty folder yields none.
        let (only_text, _) = assemble_group("b", "b", &[file(".env", "pure free text\n")]);
        let only_text = only_text.unwrap();
        assert!(only_text.variables.is_empty() && only_text.notes.contains("pure free text"));
        assert!(assemble_group("c", "c", &[file("a.json", "")]).0.is_none());
    }

    #[test]
    fn oversized_values_and_variable_counts_are_skipped_not_fatal() {
        let big = "x".repeat(MAX_VALUE_BYTES + 1);
        let env = format!("BIG={big}\nOK=1\n");
        let (group, skipped) = assemble_group("a", "a", &[file(".env", &env)]);
        let group = group.unwrap();
        assert!(group.variables.iter().all(|v| v.name != "BIG"));
        assert!(skipped
            .iter()
            .any(|s| s.name == "BIG" && s.reason == "value_too_large"));
        let many: String = (0..=MAX_VARIABLES).map(|i| format!("V{i}=1\n")).collect();
        let (group, skipped) = assemble_group("a", "a", &[file(".env", &many)]);
        assert_eq!(group.unwrap().variables.len(), MAX_VARIABLES);
        assert!(skipped.iter().any(|s| s.reason == "too_many_variables"));
    }

    /// The fixture of design section 5, with fake data only.
    fn write_fixture(root: &Path) {
        let a = root.join("服务器A");
        let b = root.join("服务器B");
        let c = root.join("充值");
        for dir in [&a, &b, &c] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(
            a.join(".env"),
            format!(
                "SSH_HOST=203.0.113.7\nSSH_PRIVATE_KEY=\"old-fake-key\"\nSSH_PRIVATE_KEY=\"{}\"\nDASHSCOPE_API_KEY=sk-fake-dashscope\nthis line is just words\n---\n",
                FAKE_KEY.replace('\n', "\\n")
            ),
        )
        .unwrap();
        fs::write(a.join("id_ed25519"), FAKE_KEY).unwrap();
        fs::write(a.join("notes.txt"), "panel notes\n").unwrap();
        fs::write(a.join("users.csv"), "name,team\nfake,ops\n").unwrap();
        fs::write(a.join("empty.json"), "").unwrap();
        fs::write(
            b.join(".env"),
            "everything here is free text\nno variables at all\n",
        )
        .unwrap();
        fs::write(b.join("key_ed25519"), OTHER_KEY).unwrap();
        fs::write(c.join(".env"), "recharge account is the second one\n").unwrap();
    }

    #[test]
    fn scans_the_documented_fixture_folder() {
        let root = tempfile::tempdir().unwrap();
        write_fixture(root.path());
        let result = scan_paths(&[root.path().to_path_buf()]).unwrap();
        let names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        // Directory order is sorted by path: 充值 (CJK) sorts after the ASCII-free names by bytes.
        assert_eq!(result.groups.len(), 3);
        for expected in ["服务器A", "服务器B", "充值"] {
            assert!(names.contains(&expected), "{expected} in {names:?}");
        }
        let a = result.groups.iter().find(|g| g.name == "服务器A").unwrap();
        assert_eq!(var(a, "SSH_HOST").values, ["203.0.113.7"]);
        let key = var(a, "SSH_PRIVATE_KEY");
        assert!(
            key.duplicate() && key.values.len() == 2,
            "{:?}",
            key.values.len()
        );
        // The key file has the same content as the second value, so it adds nothing.
        assert!(a.variables.iter().all(|v| v.name != "SSH_PRIVATE_KEY_2"));
        assert!(var(a, "DASHSCOPE_API_KEY").secret);
        assert!(a
            .notes
            .contains("来源文件：.env\nthis line is just words\n---"));
        assert!(a.notes.contains("来源文件：notes.txt\npanel notes"));
        assert!(a.notes.contains("来源文件：users.csv"));

        let b = result.groups.iter().find(|g| g.name == "服务器B").unwrap();
        assert_eq!(b.variables.len(), 1);
        assert_eq!(
            var(b, "SSH_PRIVATE_KEY").values[0],
            normalized_key(OTHER_KEY)
        );
        assert!(b.notes.contains("everything here is free text"));
        let c = result.groups.iter().find(|g| g.name == "充值").unwrap();
        assert!(c.variables.is_empty() && c.notes.contains("recharge account"));

        assert!(result
            .skipped
            .iter()
            .any(|s| s.name == "empty.json" && s.reason == "empty"));
    }

    #[test]
    fn limits_depth_file_count_size_and_binary_files() {
        let root = tempfile::tempdir().unwrap();
        // Depth: root(0) / d1 / d2 / d3 / d4. Files at depth 3 are read, depth 4 are not.
        let deep = root.path().join("d1/d2/d3/d4");
        fs::create_dir_all(&deep).unwrap();
        fs::write(root.path().join("d1/d2/d3/.env"), "AT_THREE=1\n").unwrap();
        fs::write(deep.join(".env"), "AT_FOUR=1\n").unwrap();
        fs::write(root.path().join("d1/binary.txt"), b"abc\0def").unwrap();
        fs::write(
            root.path().join("d1/big.txt"),
            vec![b'a'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        fs::create_dir_all(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".git/.env"), "IN_GIT=1\n").unwrap();
        let result = scan_paths(&[root.path().to_path_buf()]).unwrap();
        let all: Vec<&str> = result
            .groups
            .iter()
            .flat_map(|g| g.variables.iter().map(|v| v.name.as_str()))
            .collect();
        assert_eq!(all, ["AT_THREE"]);
        let reasons: Vec<&str> = result.skipped.iter().map(|s| s.reason).collect();
        assert!(reasons.contains(&"binary") && reasons.contains(&"too_large"));

        // The 200-file cap leaves the rest unread.
        let many = tempfile::tempdir().unwrap();
        for index in 0..210 {
            fs::write(many.path().join(format!("f{index:03}.txt")), "note\n").unwrap();
        }
        let result = scan_paths(&[many.path().to_path_buf()]).unwrap();
        assert_eq!(
            result
                .skipped
                .iter()
                .filter(|s| s.reason == "file_limit")
                .count(),
            10
        );
    }

    #[test]
    fn single_files_name_their_group_and_bad_paths_fail_without_echoing() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("MyServer");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(".env"), "A=1\n").unwrap();
        fs::write(folder.join("deploy.json"), "{\"B\":\"2\"}").unwrap();
        let result = scan_paths(&[folder.join(".env"), folder.join("deploy.json")]).unwrap();
        let names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["MyServer", "deploy"]);
        assert!(scan_paths(&[]).is_err());
        assert!(scan_paths(&[PathBuf::from("relative/path")]).is_err());
        let missing = root.path().join("missing");
        let error = scan_paths(&[missing]).unwrap_err();
        assert!(!error.contains("missing"));
    }

    #[test]
    fn no_value_is_in_the_debug_output_of_errors() {
        // Parse failures come back as plain text lines, never as errors that quote content.
        let parsed = parse_env_lines("PASSWORD=fake-secret-123\nnot a pair\n");
        assert_eq!(parsed.other_lines, ["not a pair"]);
    }
}
