//! Masked view of the files behind an AI import (design 2026-11-10 section 1).
//!
//! The AI gets the structure of the user's files (variable names, labels, hosts, ports, short
//! user names) but never a secret: every value that looks like one, and every value that follows
//! a password-like keyword, becomes a placeholder `«S<n>:<description>»`. The mapping from
//! placeholder number to real value lives only in this process (`Inspections`) and expires
//! after 30 minutes. When in doubt a word is masked.

use crate::scan::{self, FileInput, Kind, SkippedFile};
use regex::Regex;
use secrethub_core::groups::{is_secret_name, valid_variable_name};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_INSPECTIONS: usize = 4;
const MAX_VIEW_LINES: usize = 20_000;

const OPEN: char = '«';
const CLOSE: char = '»';

/// Words after which the next value is a secret.
const KEYWORDS: [&str; 17] = [
    "密码",
    "口令",
    "密钥",
    "秘钥",
    "令牌",
    "password",
    "passwd",
    "passphrase",
    "pwd",
    "pass",
    "key",
    "token",
    "secret",
    "apikey",
    "api-key",
    "api_key",
    "sk",
];

/// Characters that end a word. `:` and `=` stay inside words (URLs, ports, `key=value`).
const SEPARATORS: &[char] = &[
    ',', ';', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>', '|', '，', '；', '、', '（', '）',
    '：', '。', '【', '】', '“', '”', '‘', '’',
];

/// Characters between the pieces of a literal (see `piece_words`).
const PIECE_SEPARATORS: &[char] = &[
    ':', '/', '@', '?', '&', '=', '#', ',', ';', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>',
    '|', '\\', '，', '；', '：', '（', '）', '、', '«', '»',
];

/// URL schemes a literal may add on its own (to join a host and a port into an address).
pub const FREE_PIECES: [&str; 11] = [
    "http",
    "https",
    "ssh",
    "sftp",
    "ftp",
    "tcp",
    "redis",
    "mysql",
    "postgres",
    "postgresql",
    "mongodb",
];

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid pattern"))
}

fn re_email() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(
        &CELL,
        r"^[A-Za-z0-9._%+-]{1,40}@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*$",
    )
}

fn re_host() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(
        &CELL,
        r"^([A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?\.)+[A-Za-z]{2,24}(:\d{1,5})?$",
    )
}

fn re_ipv4() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"^(\d{1,3}\.){3}\d{1,3}(:\d{1,5})?(/\d{1,2})?$")
}

fn re_region() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"^[a-z]{2}(-[a-z]+)+(-\d{1,2})?$")
}

fn re_username() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"^[A-Za-z][A-Za-z0-9_.-]*$")
}

fn re_marker() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, "«S\\d+:[^»]*»")
}

fn re_url() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(&CELL, r"^([A-Za-z][A-Za-z0-9+.-]*)://(.*)$")
}

/// Marker number in `S3`, `«S3»` or `«S3:12 位»`.
pub fn parse_token_ref(reference: &str) -> Option<u32> {
    let inner = reference
        .trim()
        .trim_start_matches(OPEN)
        .trim_end_matches(CLOSE);
    let inner = inner.split(':').next()?;
    inner.strip_prefix('S')?.parse().ok().filter(|n| *n > 0)
}

// ----------------------------------------------------------------------- placeholders

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
}

/// Collects placeholders; the same value always gets the same number.
#[derive(Default)]
pub struct Masker {
    tokens: Vec<Token>,
    index: HashMap<String, usize>,
}

/// `ssh-ed25519 AAAA... comment`: one unit, so the AI can place it as a whole.
fn is_public_key(text: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    regex(
        &CELL,
        r"^(ssh-[a-z0-9-]+|ecdsa-[a-z0-9-]+)\s+[A-Za-z0-9+/=]{16,}(\s+\S+)?$",
    )
    .is_match(text.trim())
}

fn describe(value: &str, private_key: bool) -> String {
    if private_key {
        return "多行私钥".to_owned();
    }
    if is_public_key(value) {
        return "SSH 公钥".to_owned();
    }
    if value.contains('\n') {
        return "多行文本".to_owned();
    }
    let length = value.chars().count();
    let classes = [
        value.chars().any(|c| c.is_ascii_lowercase()),
        value.chars().any(|c| c.is_ascii_uppercase()),
        value.chars().any(|c| c.is_ascii_digit()),
        value.chars().any(|c| !c.is_ascii_alphanumeric()),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if length >= 12 && classes >= 2 {
        format!("{length} 位随机串")
    } else {
        format!("{length} 位")
    }
}

impl Masker {
    pub fn token(&mut self, value: &str, private_key: bool) -> String {
        let position = match self.index.get(value) {
            Some(position) => *position,
            None => {
                self.tokens.push(Token {
                    value: value.to_owned(),
                });
                self.index.insert(value.to_owned(), self.tokens.len() - 1);
                self.tokens.len() - 1
            }
        };
        format!(
            "{OPEN}S{}:{}{CLOSE}",
            position + 1,
            describe(&self.tokens[position].value, private_key)
        )
    }
}

// --------------------------------------------------------------------- word classes

fn is_keyword(word: &str) -> bool {
    let lower = word.to_lowercase();
    KEYWORDS.iter().any(|keyword| *keyword == lower)
}

/// Variable names that mark their value as a secret: the group-templates rules plus `PASS`.
pub fn name_is_secret(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    is_secret_name(name) || (!upper.contains("PUBLIC") && upper.contains("PASS"))
}

fn user_like_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "USER", "USERNAME", "ACCOUNT", "LOGIN", "EMAIL", "BUCKET", "REGION", "NAME",
    ]
    .iter()
    .any(|suffix| upper.ends_with(suffix))
}

fn digit_count(word: &str) -> usize {
    word.chars().filter(char::is_ascii_digit).count()
}

/// Short, name-like word: nothing that could be a password.
fn username_style(word: &str, relaxed: bool) -> bool {
    let limit = if relaxed { 32 } else { 11 };
    let length = word.chars().count();
    if length == 0 || length > limit || !re_username().is_match(word) {
        return false;
    }
    let mixed = word.chars().any(|c| c.is_ascii_uppercase())
        && word.chars().any(|c| c.is_ascii_lowercase())
        && digit_count(word) > 0;
    !mixed && digit_count(word) <= if relaxed { 4 } else { 2 }
}

fn plain_pattern(word: &str) -> bool {
    if word.chars().all(|c| c.is_ascii_digit()) {
        return word.len() <= 5;
    }
    if re_ipv4().is_match(word) || re_email().is_match(word) || re_host().is_match(word) {
        return true;
    }
    if word.contains(':') && word.parse::<std::net::Ipv6Addr>().is_ok() {
        return true;
    }
    if let Some(inner) = word.strip_prefix('[').and_then(|w| w.split(']').next()) {
        if inner.parse::<std::net::Ipv6Addr>().is_ok() {
            return true;
        }
    }
    if re_region().is_match(word) {
        return true;
    }
    let lower = word.to_ascii_lowercase();
    lower.starts_with("ssh-") || lower.starts_with("ecdsa-")
}

fn has_alphanumeric(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

/// A path segment of a URL: short and word-like, or masked.
fn plain_segment(segment: &str) -> bool {
    segment.is_empty()
        || (segment.chars().count() <= 11
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.~-".contains(c))
            && !(digit_count(segment) > 0
                && segment.chars().any(|c| c.is_ascii_alphabetic())
                && segment.chars().count() >= 6))
}

// -------------------------------------------------------------------------- the masker

#[derive(Clone, Copy, Default)]
struct Ctx {
    /// The variable name suggests a user or account name (longer plain words are fine).
    user_like: bool,
}

impl Masker {
    /// One URL: userinfo and secret query parameters become placeholders.
    fn url(&mut self, scheme: &str, rest: &str) -> String {
        let (authority_and_path, fragment) = match rest.split_once('#') {
            Some((head, tail)) => (head, Some(tail)),
            None => (rest, None),
        };
        let (location, query) = match authority_and_path.split_once('?') {
            Some((head, tail)) => (head, Some(tail)),
            None => (authority_and_path, None),
        };
        let (authority, path) = match location.find('/') {
            Some(position) => (&location[..position], &location[position..]),
            None => (location, ""),
        };
        let mut out = format!("{scheme}://");
        let host = match authority.rsplit_once('@') {
            Some((userinfo, host)) => {
                match userinfo.split_once(':') {
                    Some((user, password)) => {
                        out.push_str(&self.plain_or_token(user, Ctx { user_like: true }));
                        out.push(':');
                        out.push_str(&self.token(password, false));
                    }
                    None => out.push_str(&self.plain_or_token(userinfo, Ctx { user_like: true })),
                }
                out.push('@');
                host
            }
            None => authority,
        };
        out.push_str(&self.plain_or_token(host, Ctx::default()));
        for segment in path.split('/').enumerate() {
            if segment.0 > 0 {
                out.push('/');
            }
            if plain_segment(segment.1) {
                out.push_str(segment.1);
            } else {
                out.push_str(&self.token(segment.1, false));
            }
        }
        if let Some(query) = query {
            out.push('?');
            let pairs: Vec<String> = query
                .split('&')
                .map(|pair| match pair.split_once('=') {
                    Some((name, value)) => {
                        if value.is_empty() {
                            pair.to_owned()
                        } else if name_is_secret(name)
                            || is_keyword(name)
                            || !username_style(value, false) && !plain_pattern(value)
                        {
                            format!("{name}={}", self.token(value, false))
                        } else {
                            pair.to_owned()
                        }
                    }
                    None => self.plain_or_token(pair, Ctx::default()),
                })
                .collect();
            out.push_str(&pairs.join("&"));
        }
        if let Some(fragment) = fragment {
            out.push('#');
            out.push_str(&self.plain_or_token(fragment, Ctx::default()));
        }
        out
    }

    /// A single ASCII word: plain when it matches a harmless shape, a placeholder otherwise.
    fn plain_or_token(&mut self, word: &str, ctx: Ctx) -> String {
        if word.is_empty() || word.starts_with(OPEN) {
            return word.to_owned();
        }
        if let Some(captures) = re_url().captures(word) {
            let (scheme, rest) = (captures[1].to_owned(), captures[2].to_owned());
            return self.url(&scheme, &rest);
        }
        if plain_pattern(word) || username_style(word, ctx.user_like) || !has_alphanumeric(word) {
            return word.to_owned();
        }
        self.token(word, false)
    }

    /// Masks free text word by word. Returns the masked text.
    pub fn text(&mut self, text: &str, ctx_user_like: bool) -> String {
        let ctx = Ctx {
            user_like: ctx_user_like,
        };
        let mut out = String::with_capacity(text.len());
        // A keyword was seen: the next word that carries content is a secret.
        let mut expect_secret = false;
        let mut characters = text.char_indices().peekable();
        while let Some((start, ch)) = characters.next() {
            if ch.is_whitespace() || SEPARATORS.contains(&ch) {
                out.push(ch);
                continue;
            }
            if ch == OPEN {
                // An existing placeholder passes through untouched.
                let mut end = start + ch.len_utf8();
                for (position, next) in characters.by_ref() {
                    end = position + next.len_utf8();
                    if next == CLOSE {
                        break;
                    }
                }
                out.push_str(&text[start..end]);
                expect_secret = false;
                continue;
            }
            // Collect a run of the same script class; after a keyword the whole chunk counts.
            let ascii = ch.is_ascii();
            let mut end = start + ch.len_utf8();
            while let Some(&(position, next)) = characters.peek() {
                if next.is_whitespace()
                    || SEPARATORS.contains(&next)
                    || next == OPEN
                    || (!expect_secret && next.is_ascii() != ascii)
                {
                    break;
                }
                end = position + next.len_utf8();
                characters.next();
            }
            let run = &text[start..end];
            if expect_secret {
                if matches!(run, "is" | "-" | "->" | "=" | "=>" | "是" | "为")
                    || !has_alphanumeric(run)
                {
                    out.push_str(run);
                } else {
                    expect_secret = false;
                    let trimmed = run.trim_end_matches([':', '.', '=', '!', '?']);
                    out.push_str(&self.token(trimmed, false));
                    out.push_str(&run[trimmed.len()..]);
                }
                continue;
            }
            if ascii {
                out.push_str(&self.ascii_run(run, ctx, &mut expect_secret));
            } else {
                out.push_str(&self.other_run(run, &mut expect_secret));
            }
        }
        out
    }

    /// Non-ASCII run (labels in Chinese etc.): visible, but a keyword inside it arms the mask.
    fn other_run(&mut self, run: &str, expect_secret: &mut bool) -> String {
        if *expect_secret && has_alphanumeric(run) {
            *expect_secret = false;
            return self.token(run, false);
        }
        let last_keyword = KEYWORDS
            .iter()
            .filter(|keyword| !keyword.is_ascii())
            .filter_map(|keyword| run.rfind(keyword).map(|at| (at, keyword.len())))
            .max();
        if let Some((at, length)) = last_keyword {
            let tail = &run[at + length..];
            let value = tail.trim_start_matches(['是', '为', '：', ':', '=', ' ']);
            if value.is_empty() {
                *expect_secret = true;
                return run.to_owned();
            }
            let prefix = &run[..run.len() - value.len()];
            return format!("{prefix}{}", self.token(value, false));
        }
        run.to_owned()
    }

    fn ascii_run(&mut self, run: &str, ctx: Ctx, expect_secret: &mut bool) -> String {
        // Trailing punctuation stays visible and is not part of the word.
        let trimmed = run.trim_end_matches([':', '.', '=', '!', '?']);
        let tail = &run[trimmed.len()..];
        let head_len = trimmed.len() - trimmed.trim_start_matches(['=']).len();
        let (lead, word) = (&trimmed[..head_len], &trimmed[head_len..]);
        let mut out = String::from(lead);
        if word.is_empty() {
            out.push_str(tail);
            return out;
        }
        if *expect_secret {
            if matches!(word, "is" | "-" | "->" | "=" | "=>") || !has_alphanumeric(word) {
                out.push_str(word);
                out.push_str(tail);
                return out;
            }
            *expect_secret = false;
            out.push_str(&self.token(word, false));
            out.push_str(tail);
            return out;
        }
        // `key=value` / `keyword:value` inside a run.
        if !word.contains("://") {
            if let Some((left, right)) = word.split_once('=').or_else(|| {
                word.split_once(':')
                    .filter(|(left, _)| is_keyword(left) || secret_variable(left))
            }) {
                let separator = word.as_bytes()[left.len()] as char;
                let secret_left = is_keyword(left) || secret_variable(left);
                out.push_str(&self.plain_or_token(left, ctx));
                out.push(separator);
                if right.is_empty() {
                    *expect_secret = secret_left;
                } else if secret_left {
                    out.push_str(&self.token(right, false));
                } else {
                    out.push_str(&self.plain_or_token(right, ctx));
                }
                out.push_str(tail);
                return out;
            }
        }
        if is_keyword(word) || secret_variable(word) {
            out.push_str(word);
            out.push_str(tail);
            *expect_secret = true;
            return out;
        }
        out.push_str(&self.plain_or_token(word, ctx));
        out.push_str(tail);
        out
    }

    /// The value of `NAME=value`: one placeholder when the name says secret, word-wise otherwise.
    fn value(&mut self, name: &str, value: &str) -> String {
        if value.trim().is_empty() {
            return String::new();
        }
        if value.trim_start().starts_with(OPEN) && !value.contains('\n') {
            return value.trim().to_owned();
        }
        if is_public_key(value) {
            return self.token(value.trim(), false);
        }
        if name_is_secret(name) {
            return self.token(value.trim_end_matches(['\r']), false);
        }
        self.text(value, user_like_name(name))
    }
}

/// `SOME_PASSWORD`-style words in free text count as keywords.
fn secret_variable(word: &str) -> bool {
    word.contains('_') && valid_variable_name(word) && name_is_secret(word)
}

// ---------------------------------------------------------------------- whole files

/// Replaces PEM private key blocks by placeholders (and neutralises `«`/`»` in the file).
fn hide_key_blocks(masker: &mut Masker, text: &str) -> String {
    let text = text.replace(OPEN, "<").replace(CLOSE, ">");
    let mut out = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let marks_start = line.contains("-----BEGIN") && line.contains("PRIVATE KEY");
        if !marks_start {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Text before the BEGIN marker (`KEY="-----BEGIN ...`) stays.
        let begin = line.find("-----BEGIN").unwrap_or(0);
        let mut block = vec![line[begin..].to_owned()];
        let mut closed = line.contains("-----END");
        let mut suffix = String::new();
        if closed {
            if let Some(end) = line.rfind("-----") {
                suffix = line[end + 5..].to_owned();
            }
        }
        while !closed {
            let Some(next) = lines.next() else { break };
            if next.contains("-----END") {
                closed = true;
                let end = next.rfind("-----").map_or(next.len(), |at| at + 5);
                block.push(next[..end].to_owned());
                suffix = next[end..].to_owned();
            } else {
                block.push(next.to_owned());
            }
        }
        let key = format!("{}\n", block.join("\n").trim());
        out.push_str(&line[..begin]);
        out.push_str(&masker.token(&key, true));
        out.push_str(&suffix);
        out.push('\n');
    }
    out
}

fn kv_line(masker: &mut Masker, name: &str, value: &str) -> String {
    format!("{name}={}", masker.value(name, value))
}

fn mask_lines(masker: &mut Masker, text: &str, allow_kv: bool) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        if allow_kv && !line.trim_start().starts_with('#') {
            if let Some((name, value, next)) = scan::parse_env_assignment(&lines, index) {
                out.push(kv_line(masker, &name, &value));
                index = next;
                continue;
            }
        }
        if is_public_key(line) {
            out.push(masker.token(line.trim(), false));
        } else {
            out.push(masker.text(line.trim_end(), false));
        }
        index += 1;
    }
    out
}

const CSV_SECRET_HEADERS: [&str; 14] = [
    "password", "pass", "pwd", "secret", "token", "key", "cvv", "cvc", "pin", "otp", "totp",
    "密码", "卡号", "口令",
];

fn mask_csv(masker: &mut Masker, text: &str) -> Vec<String> {
    let rows = crate::csv_import::parse_csv(text);
    let header: Vec<String> = rows
        .first()
        .map(|row| row.iter().map(|cell| cell.trim().to_lowercase()).collect())
        .unwrap_or_default();
    let secret_column: Vec<bool> = header
        .iter()
        .map(|name| {
            CSV_SECRET_HEADERS
                .iter()
                .any(|keyword| name.contains(keyword))
        })
        .collect();
    let mut out = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let cell = cell.trim();
                if cell.is_empty() {
                    String::new()
                } else if row_index == 0 {
                    masker.text(cell, false)
                } else if secret_column.get(column).copied().unwrap_or(false) {
                    masker.token(cell, false)
                } else {
                    masker.text(cell, header.get(column).is_some_and(|h| user_like_name(h)))
                }
            })
            .collect();
        out.push(cells.join(","));
    }
    out
}

/// One masked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskedFile {
    pub name: String,
    pub lines: Vec<String>,
}

fn mask_file(masker: &mut Masker, file: &FileInput) -> Option<MaskedFile> {
    if file.text.trim().is_empty() {
        return None;
    }
    let hidden = hide_key_blocks(masker, &file.text);
    let lines = match scan::classify(file) {
        Kind::PrivateKey => {
            let key = scan::normalized_key(&file.text);
            vec![masker.token(&key, true)]
        }
        Kind::Unsupported => return None,
        Kind::PublicKey => mask_lines(masker, &hidden, false),
        Kind::Env => mask_lines(masker, &hidden, true),
        Kind::Json => match scan::json_variables(&file.text) {
            Some(entries) if !entries.is_empty() => entries
                .iter()
                .map(|(name, value)| kv_line(masker, name, value))
                .collect(),
            _ => mask_lines(masker, &hidden, false),
        },
        Kind::Text => {
            if file.name.to_ascii_lowercase().ends_with(".csv") {
                mask_csv(masker, &file.text.replace(OPEN, "<").replace(CLOSE, ">"))
            } else {
                mask_lines(masker, &hidden, true)
            }
        }
    };
    Some(MaskedFile {
        name: file.name.clone(),
        lines,
    })
}

// ------------------------------------------------------------------ inspection store

/// The words of a text, split like literals are: the visible words of a view and the pieces of a
/// literal are compared with this.
pub fn piece_words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || PIECE_SEPARATORS.contains(&c))
        .filter(|piece| !piece.is_empty())
        .map(str::to_owned)
        .collect()
}

pub struct Inspection {
    pub files: Vec<MaskedFile>,
    pub skipped: Vec<SkippedFile>,
    pub truncated: bool,
    tokens: Vec<Token>,
    words: HashSet<String>,
    created: Instant,
}

impl Inspection {
    /// Masks the files. Placeholder numbers are stable within one inspection.
    pub fn build(files: &[FileInput], skipped: Vec<SkippedFile>) -> Self {
        Self::build_at(files, skipped, Instant::now())
    }

    pub fn build_at(files: &[FileInput], skipped: Vec<SkippedFile>, now: Instant) -> Self {
        let mut masker = Masker::default();
        let mut masked = Vec::new();
        let mut budget = MAX_VIEW_LINES;
        let mut truncated = false;
        for file in files {
            let Some(mut view) = mask_file(&mut masker, file) else {
                continue;
            };
            if view.lines.len() > budget {
                view.lines.truncate(budget);
                truncated = true;
            }
            budget -= view.lines.len();
            masked.push(view);
        }
        let mut words = HashSet::new();
        for file in &masked {
            for line in &file.lines {
                words.extend(piece_words(&re_marker().replace_all(line, " ")));
            }
        }
        Self {
            files: masked,
            skipped,
            truncated,
            tokens: masker.tokens,
            words,
            created: now,
        }
    }

    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created) >= TOKEN_TTL
    }

    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Real value behind a placeholder number.
    pub fn token_value(&self, number: u32) -> Option<&str> {
        self.tokens
            .get((number as usize).checked_sub(1)?)
            .map(|token| token.value.as_str())
    }

    /// Every piece of `literal` must be a word of the view (or a free URL scheme).
    pub fn literal_allowed(&self, literal: &str) -> bool {
        if literal.is_empty() || literal.contains(OPEN) || literal.contains(CLOSE) {
            return false;
        }
        let pieces = piece_words(literal);
        !pieces.is_empty()
            && pieces
                .iter()
                .all(|piece| self.words.contains(piece) || FREE_PIECES.contains(&piece.as_str()))
    }

    /// True when `text` repeats a real value or contains something that would have been masked.
    pub fn text_leaks_value(&self, text: &str) -> bool {
        if self.tokens.iter().any(|token| {
            token.value.trim().chars().count() >= 3 && text.contains(token.value.trim())
        }) {
            return true;
        }
        text.split(|c: char| c.is_whitespace() || SEPARATORS.contains(&c) || c == '/' || c == ',')
            .filter(|word| word.is_ascii() && word.chars().count() >= 12)
            .any(|word| {
                let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
                let classes = [
                    word.chars().any(|c| c.is_ascii_lowercase()),
                    word.chars().any(|c| c.is_ascii_uppercase()),
                    word.chars().any(|c| c.is_ascii_digit()),
                    word.chars()
                        .any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '-'),
                ]
                .into_iter()
                .filter(|present| *present)
                .count();
                classes >= 3
            })
    }
}

/// In-memory inspections, newest four, each valid for `TOKEN_TTL`.
#[derive(Default)]
pub struct Inspections {
    inner: Mutex<HashMap<String, std::sync::Arc<Inspection>>>,
}

impl Inspections {
    pub fn insert(&self, id: String, inspection: Inspection) {
        if let Ok(mut map) = self.inner.lock() {
            map.retain(|_, item| !item.expired(Instant::now()));
            while map.len() >= MAX_INSPECTIONS {
                let Some(oldest) = map
                    .iter()
                    .min_by_key(|(_, item)| item.created)
                    .map(|(id, _)| id.clone())
                else {
                    break;
                };
                map.remove(&oldest);
            }
            map.insert(id, std::sync::Arc::new(inspection));
        }
    }

    /// `None` when the id is unknown or the inspection expired.
    pub fn get(&self, id: &str, now: Instant) -> Option<std::sync::Arc<Inspection>> {
        let mut map = self.inner.lock().ok()?;
        map.retain(|_, item| !item.expired(now));
        map.get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> FileInput {
        FileInput {
            name: name.to_owned(),
            text: text.to_owned(),
        }
    }

    fn view(files: &[FileInput]) -> (Inspection, String) {
        let inspection = Inspection::build(files, Vec::new());
        let text = inspection
            .files
            .iter()
            .flat_map(|f| f.lines.clone())
            .collect::<Vec<_>>()
            .join("\n");
        (inspection, text)
    }

    fn masked(text: &str) -> String {
        view(&[file("a/.env", text)]).1
    }

    #[test]
    fn plain_things_stay_visible() {
        let text = masked(
            "SSH_HOST=8.0.0.1\nSSH_PORT=22\nSSH_USER=root\nDB_HOST=db.example.com\nREGION=cn-hangzhou\nMAIL=ops@example.com\nV6=2001:db8::1\nURL=https://8.0.0.1:8888/panel\nHOST_PORT=1.2.3.4:3306\n",
        );
        for expected in [
            "SSH_HOST=8.0.0.1",
            "SSH_PORT=22",
            "SSH_USER=root",
            "DB_HOST=db.example.com",
            "REGION=cn-hangzhou",
            "MAIL=ops@example.com",
            "V6=2001:db8::1",
            "URL=https://8.0.0.1:8888/panel",
            "HOST_PORT=1.2.3.4:3306",
        ] {
            assert!(text.contains(expected), "{expected} in\n{text}");
        }
        assert!(!text.contains('«'), "{text}");
    }

    #[test]
    fn secrets_become_placeholders_by_name_keyword_length_and_doubt() {
        let text = masked(
            "SSH_PASSWORD=Sup3rSecret\nAPI_TOKEN=abc\nPLAIN_LONG=Xk3jf93hdk3kdjf93\nSHORT_ODD=Pa$$1\nPIN=123456\nNOTE=the quick brown fox\n",
        );
        for leaked in ["Sup3rSecret", "abc", "Xk3jf93hdk3kdjf93", "Pa$$1", "123456"] {
            assert!(!text.contains(leaked), "{leaked} leaked in\n{text}");
        }
        assert!(
            text.contains("SSH_PASSWORD=«S1:12 位随机串»") || text.contains("SSH_PASSWORD=«S1:")
        );
        assert!(text.contains("NOTE=the quick brown fox"), "{text}");
    }

    #[test]
    fn values_after_keywords_in_free_text_are_masked_but_labels_stay() {
        let text = masked(
            "阿里宝塔 8.0.0.1 用户 root 密码 Abc12345\n账号：admin 密码：张三123\npassword: hunter2\npwd=1234 and key abcdef\nSSH 密码是Zz9\n",
        );
        for leaked in ["Abc12345", "张三123", "hunter2", "1234 ", "abcdef", "Zz9"] {
            assert!(!text.contains(leaked), "{leaked} leaked in\n{text}");
        }
        for kept in ["阿里宝塔", "8.0.0.1", "root", "admin", "密码"] {
            assert!(text.contains(kept), "{kept} missing in\n{text}");
        }
    }

    #[test]
    fn urls_lose_userinfo_secret_queries_and_random_paths() {
        let text = masked(
            "A=mysql://dbuser:P4ssw0rd!x@10.0.0.2:3306/app\nB=https://host.example.com/api?token=abc123XYZ&mode=fast\nC=https://1.2.3.4:8888/a1B2c3D4e5F6g7\n",
        );
        for leaked in ["P4ssw0rd!x", "abc123XYZ", "a1B2c3D4e5F6g7"] {
            assert!(!text.contains(leaked), "{leaked} leaked in\n{text}");
        }
        assert!(text.contains("mysql://dbuser:«S"), "{text}");
        assert!(text.contains("@10.0.0.2:3306/app"), "{text}");
        assert!(text.contains("mode=fast"), "{text}");
        assert!(text.contains("token=«S"), "{text}");
    }

    #[test]
    fn private_keys_become_one_placeholder_everywhere() {
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEKEYLINE1\nFAKEKEYLINE2\n-----END OPENSSH PRIVATE KEY-----";
        let (inspection, text) = view(&[
            file("a/.env", &format!("SSH_PRIVATE_KEY=\"{key}\"\nOTHER=1\n")),
            file("a/id_ed25519", &format!("{key}\n")),
            file("a/notes.txt", &format!("my key:\n{key}\nend")),
        ]);
        assert!(!text.contains("FAKEKEYLINE"), "{text}");
        assert!(text.contains("SSH_PRIVATE_KEY=«S1:多行私钥»"), "{text}");
        assert_eq!(inspection.token_count(), 1, "same key, same placeholder");
        assert!(inspection
            .token_value(1)
            .unwrap()
            .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(text.contains("OTHER=1"));
        assert!(text.contains("end"));
    }

    #[test]
    fn public_keys_are_one_placeholder_in_env_files_and_key_files() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFAKE fake@host";
        let (inspection, text) = view(&[
            file("a/.env", &format!("PUB={key}\n")),
            file("a/id.pub", &format!("{key}\n")),
        ]);
        assert!(!text.contains("AAAAC3NzaC1"), "{text}");
        assert_eq!(text, "PUB=«S1:SSH 公钥»\n«S1:SSH 公钥»");
        assert_eq!(inspection.token_value(1), Some(key));
    }

    #[test]
    fn same_value_same_placeholder_and_lookup_works() {
        let (inspection, text) = view(&[file(
            "a/.env",
            "A_PASSWORD=same-secret-value\nB_PASSWORD=same-secret-value\nC_PASSWORD=another-one\n",
        )]);
        assert_eq!(inspection.token_count(), 2);
        assert!(text.contains("A_PASSWORD=«S1:") && text.contains("B_PASSWORD=«S1:"));
        assert_eq!(inspection.token_value(2), Some("another-one"));
        assert_eq!(inspection.token_value(0), None);
        assert_eq!(inspection.token_value(3), None);
    }

    #[test]
    fn files_cannot_forge_placeholders() {
        let (inspection, text) = view(&[file("a/.env", "FAKE=«S9:12 位»\n")]);
        assert_eq!(inspection.token_value(9), None);
        assert!(!text.contains('«') || text.contains("«S1:"), "{text}");
    }

    #[test]
    fn csv_password_columns_are_masked() {
        let (_, text) = view(&[file(
            "x/pw.csv",
            "name,url,username,password\nGitHub,https://github.com,octo,Hunter2pass\n",
        )]);
        assert!(!text.contains("Hunter2pass"), "{text}");
        assert!(text.contains("https://github.com"), "{text}");
        assert!(text.contains("octo"), "{text}");
    }

    #[test]
    fn literals_must_be_built_from_visible_words() {
        let (inspection, _) = view(&[file(
            "a/.env",
            "SSH_HOST=8.0.0.1\nBT_PORT=8888\nBT_PASSWORD=Zz9-secret-value\n",
        )]);
        assert!(inspection.literal_allowed("8.0.0.1"));
        assert!(inspection.literal_allowed("https://8.0.0.1:8888"));
        assert!(!inspection.literal_allowed("9.9.9.9"));
        assert!(!inspection.literal_allowed("Zz9-secret-value"));
        assert!(!inspection.literal_allowed("https://8.0.0.1:8888/made-up"));
        assert!(!inspection.literal_allowed("«S1»"));
        assert!(!inspection.literal_allowed(""));
    }

    #[test]
    fn descriptions_must_not_carry_values() {
        let (inspection, _) = view(&[file("a/.env", "A_PASSWORD=hunter2hunter2\n")]);
        assert!(inspection.text_leaks_value("password is hunter2hunter2"));
        assert!(inspection.text_leaks_value("token Xk3jf93hdK3kdjF93!"));
        assert!(
            !inspection.text_leaks_value("SSH 登录密码，用于 SSH_PRIVATE_KEY_FALLBACK 之外的登录")
        );
    }

    #[test]
    fn token_references_parse_in_all_spellings() {
        assert_eq!(parse_token_ref("S3"), Some(3));
        assert_eq!(parse_token_ref("«S3»"), Some(3));
        assert_eq!(parse_token_ref("«S12:多行私钥»"), Some(12));
        assert_eq!(parse_token_ref("S0"), None);
        assert_eq!(parse_token_ref("x"), None);
    }

    #[test]
    fn inspections_expire_after_thirty_minutes() {
        let store = Inspections::default();
        let start = Instant::now();
        store.insert(
            "inspect-1".into(),
            Inspection::build_at(&[file("a/.env", "A=1\n")], Vec::new(), start),
        );
        assert!(store
            .get("inspect-1", start + Duration::from_secs(60))
            .is_some());
        assert!(store
            .get("inspect-1", start + TOKEN_TTL + Duration::from_secs(1))
            .is_none());
    }
}
