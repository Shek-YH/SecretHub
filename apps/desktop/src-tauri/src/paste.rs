//! Smart paste (design section 3.4): recognises what a pasted text contains using local rules
//! only. `KEY=VALUE` lines become a config group, account + password lines become logins,
//! Luhn-valid 12-19 digit numbers become bank cards, and whatever is left becomes one secure
//! note. Nothing leaves the process and no AI is involved.

use crate::csv_import::normalize_expiry;
use crate::scan::{self, Extra, ScannedGroup, SkippedFile};
use regex::Regex;
use secrethub_core::items::{host_of, json_item_type, luhn_valid, validate_fields};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Largest text handled; the review of a bigger paste would be unreadable anyway.
pub const MAX_PASTE_BYTES: usize = 1024 * 1024;
const NOTE_LIMIT: usize = 100_000;

pub struct PasteResult {
    pub group: Option<ScannedGroup>,
    pub extras: Vec<Extra>,
    pub skipped: Vec<SkippedFile>,
}

struct Patterns {
    user: Regex,
    pass: Regex,
    url_label: Regex,
    bank: Regex,
    branch: Regex,
    holder: Regex,
    expiry: Regex,
    inline: Regex,
    url: Regex,
    digits: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let labeled = |labels: &str| {
            Regex::new(&format!(r"(?i)^\s*(?:{labels})\s*[:：=]\s*(.+?)\s*$")).expect("valid pattern")
        };
        Patterns {
            user: labeled(r"账号|帐号|账户|帐户|用户名|用户|登录名|登录账号|邮箱|e-?mail|user\s*name|user|account|login"),
            pass: labeled(r"密码|口令|password|passwd|pwd|pass"),
            url_label: labeled(r"网址|网站|地址|链接|url|site|website|link|host"),
            bank: labeled(r"银行|bank"),
            branch: labeled(r"开户行|branch"),
            holder: labeled(r"持卡人|holder|cardholder"),
            expiry: labeled(r"有效期|expiry|exp|valid\s*thru"),
            inline: Regex::new(
                r"(?i)(?:账号|帐号|账户|帐户|用户名|邮箱|e-?mail|user\s*name|user|account)\s*[:：]?\s*(\S+)\s+(?:密码|口令|password|passwd|pwd)\s*[:：]?\s*(\S+)",
            )
            .expect("valid pattern"),
            url: Regex::new(r"https?://[^\s,;，；]+").expect("valid pattern"),
            digits: Regex::new(r"(?:^|[^\d])((?:\d[ -]?){11,18}\d)(?:[^\d]|$)").expect("valid pattern"),
        }
    })
}

#[derive(Default)]
struct Block {
    url: Option<String>,
    user: Option<String>,
    pass: Option<String>,
    bank: Option<String>,
    branch: Option<String>,
    holder: Option<String>,
    expiry: Option<String>,
    cards: Vec<String>,
    lines: Vec<usize>,
}

impl Block {
    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// A line that looks like `label=value` for an account label must not be taken as a variable.
fn account_label(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "user"
            | "username"
            | "account"
            | "email"
            | "login"
            | "password"
            | "passwd"
            | "pwd"
            | "pass"
            | "url"
            | "site"
            | "website"
            | "link"
            | "host"
            | "bank"
            | "branch"
            | "holder"
            | "expiry"
            | "exp"
    )
}

fn card_numbers(line: &str) -> Vec<String> {
    patterns()
        .digits
        .captures_iter(line)
        .filter_map(|capture| {
            let digits: String = capture[1].chars().filter(char::is_ascii_digit).collect();
            ((12..=19).contains(&digits.len()) && luhn_valid(&digits)).then_some(digits)
        })
        .collect()
}

fn clean_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches(['.', ')', '）']).to_owned();
    host_of(&trimmed).map(|_| trimmed)
}

/// Feeds one non-blank line into the block. Returns true when the line was understood.
fn absorb(block: &mut Block, line: &str, index: usize) -> bool {
    let p = patterns();
    let mut understood = false;
    if let Some(capture) = p.inline.captures(line) {
        block.user = Some(capture[1].to_owned());
        block.pass = Some(capture[2].to_owned());
        understood = true;
    } else if let Some(capture) = p.user.captures(line) {
        block.user = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(capture) = p.pass.captures(line) {
        block.pass = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(capture) = p.url_label.captures(line) {
        if let Some(url) = clean_url(&capture[1]) {
            block.url = Some(url);
            understood = true;
        }
    } else if let Some(capture) = p.bank.captures(line) {
        block.bank = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(capture) = p.branch.captures(line) {
        block.branch = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(capture) = p.holder.captures(line) {
        block.holder = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(capture) = p.expiry.captures(line) {
        block.expiry = Some(capture[1].to_owned());
        understood = true;
    } else if let Some(found) = p.url.find(line) {
        if block.url.is_none() {
            if let Some(url) = clean_url(found.as_str()) {
                block.url = Some(url);
                understood = true;
            }
        }
    }
    let cards = card_numbers(line);
    if !cards.is_empty() {
        block.cards.extend(cards);
        understood = true;
    }
    if understood {
        block.lines.push(index);
    }
    understood
}

fn finish_block(
    block: &mut Block,
    source: &str,
    extras: &mut Vec<Extra>,
    consumed: &mut Vec<usize>,
    counter: &mut usize,
) {
    let block = std::mem::take(block);
    if block.is_empty() {
        return;
    }
    let mut used = false;
    if let Some(user) = &block.user {
        let mut fields = BTreeMap::new();
        fields.insert("username".to_owned(), user.clone());
        if let Some(pass) = &block.pass {
            fields.insert("password".to_owned(), pass.clone());
        }
        if let Some(url) = &block.url {
            fields.insert("website".to_owned(), url.clone());
        }
        if let Some(validated) =
            json_item_type("login").and_then(|def| validate_fields(def, &fields).ok())
        {
            *counter += 1;
            let name = validated
                .get("website")
                .and_then(|site| host_of(site))
                .unwrap_or_else(|| format!("粘贴的账号 {counter}"));
            extras.push(Extra {
                source: source.to_owned(),
                kind: "login",
                name,
                fields: validated,
            });
            used = true;
        }
    }
    for number in &block.cards {
        let mut fields = BTreeMap::new();
        fields.insert("card_number".to_owned(), number.clone());
        fields.insert(
            "bank".to_owned(),
            block.bank.clone().unwrap_or_else(|| "待补充".to_owned()),
        );
        if let Some(holder) = &block.holder {
            fields.insert("holder".to_owned(), holder.clone());
        }
        if let Some(branch) = &block.branch {
            fields.insert("branch".to_owned(), branch.clone());
        }
        if let Some(expiry) = block.expiry.as_deref().and_then(normalize_expiry) {
            fields.insert("expiry".to_owned(), expiry);
        }
        if let Some(validated) =
            json_item_type("bank_card").and_then(|def| validate_fields(def, &fields).ok())
        {
            let last4: String = number
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            extras.push(Extra {
                source: source.to_owned(),
                kind: "bank_card",
                name: format!(
                    "{} 尾号 {last4}",
                    validated.get("bank").cloned().unwrap_or_default()
                ),
                fields: validated,
            });
            used = true;
        }
    }
    if used {
        consumed.extend(block.lines);
    }
}

/// Recognises the pasted text. Always yields at least one candidate for non-blank text.
pub fn smart_candidates(text: &str, source: &str) -> PasteResult {
    let text: String = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut extras = Vec::new();
    let mut consumed: Vec<usize> = Vec::new();
    let mut block = Block::default();
    let mut counter = 0usize;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            finish_block(&mut block, source, &mut extras, &mut consumed, &mut counter);
            index += 1;
            continue;
        }
        if line.starts_with('#') {
            index += 1;
            continue;
        }
        // Variables first, unless the name is an account label such as `password=...`.
        if let Some((name, value, next)) = scan::parse_env_assignment(&lines, index) {
            if !account_label(&name) && !value.trim().is_empty() {
                finish_block(&mut block, source, &mut extras, &mut consumed, &mut counter);
                entries.push((name, value));
                consumed.extend(index..next);
                index = next;
                continue;
            }
        }
        // A new website line starts a new account once the current one has credentials.
        let has_url = patterns().url.is_match(line) || patterns().url_label.is_match(line);
        let starts_new = has_url && (block.user.is_some() || block.pass.is_some());
        if starts_new {
            finish_block(&mut block, source, &mut extras, &mut consumed, &mut counter);
        }
        absorb(&mut block, line, index);
        index += 1;
    }
    finish_block(&mut block, source, &mut extras, &mut consumed, &mut counter);

    let mut skipped = Vec::new();
    let (group, group_skipped) = scan::group_from_entries(source, "粘贴的配置", entries);
    skipped.extend(group_skipped);

    // Everything that was not understood stays together as one secure note.
    let leftover: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(position, line)| !consumed.contains(position) && !line.trim().is_empty())
        .map(|(_, line)| line.trim_end())
        .collect();
    let mut body = leftover.join("\n");
    if body.chars().count() > NOTE_LIMIT {
        body = body.chars().take(NOTE_LIMIT).collect();
        skipped.push(SkippedFile {
            name: source.to_owned(),
            reason: "notes_truncated",
        });
    }
    if !body.trim().is_empty() {
        let mut fields = BTreeMap::new();
        fields.insert("body".to_owned(), body);
        extras.push(Extra {
            source: source.to_owned(),
            kind: "note",
            name: "粘贴的文本".to_owned(),
            fields,
        });
    }
    PasteResult {
        group,
        extras,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extra<'a>(result: &'a PasteResult, kind: &str) -> Vec<&'a Extra> {
        result
            .extras
            .iter()
            .filter(|extra| extra.kind == kind)
            .collect()
    }

    #[test]
    fn key_value_lines_become_one_group_and_account_labels_do_not() {
        let result = smart_candidates(
            "API_HOST=api.example.com\nexport DB_PASSWORD=\"fake db pass\"\npassword=not-a-variable\nuser=someone\n",
            "粘贴",
        );
        let group = result.group.as_ref().unwrap();
        assert_eq!(group.name, "粘贴的配置");
        let names: Vec<_> = group
            .variables
            .iter()
            .map(|v| (v.name.as_str(), v.secret))
            .collect();
        assert_eq!(names, [("API_HOST", false), ("DB_PASSWORD", true)]);
        assert_eq!(group.variables[1].values[0], "fake db pass");
        // The lowercase labels form a login instead.
        let logins = extra(&result, "login");
        assert_eq!(logins.len(), 1);
        assert_eq!(logins[0].fields["username"], "someone");
        assert_eq!(logins[0].fields["password"], "not-a-variable");
    }

    #[test]
    fn chinese_and_english_account_password_lines_become_logins() {
        let text = "网址：https://login.example.com/\n账号：fake-user@example.com\n密码：fake-pass-1\n\nusername: second-user\npassword: fake-pass-2\n\n邮箱 third@example.com 密码 fake-pass-3\n\n用户名: lonely-user\n";
        let result = smart_candidates(text, "粘贴");
        let logins = extra(&result, "login");
        assert_eq!(logins.len(), 4);
        assert_eq!(logins[0].name, "login.example.com");
        assert_eq!(logins[0].fields["website"], "https://login.example.com/");
        assert_eq!(logins[0].fields["username"], "fake-user@example.com");
        assert_eq!(logins[0].fields["password"], "fake-pass-1");
        assert_eq!(logins[1].fields["username"], "second-user");
        assert_eq!(logins[1].name, "粘贴的账号 2");
        assert_eq!(logins[2].fields["username"], "third@example.com");
        assert_eq!(logins[2].fields["password"], "fake-pass-3");
        assert!(!logins[3].fields.contains_key("password"));
        assert!(
            extra(&result, "note").is_empty(),
            "everything was understood"
        );
    }

    #[test]
    fn a_website_line_after_credentials_starts_the_next_account() {
        let text = "https://a.example.com\n账号：user-a\n密码：pass-a\nhttps://b.example.com\n账号：user-b\n密码：pass-b\n";
        let result = smart_candidates(text, "粘贴");
        let logins = extra(&result, "login");
        assert_eq!(logins.len(), 2);
        assert_eq!(logins[0].fields["website"], "https://a.example.com");
        assert_eq!(logins[1].fields["website"], "https://b.example.com");
        assert_eq!(logins[1].fields["username"], "user-b");
    }

    #[test]
    fn only_luhn_valid_twelve_to_nineteen_digit_numbers_are_cards() {
        let text = "银行：Fake Bank\n卡号：4111 1111 1111 1111\n持卡人：Fake Holder\n有效期：09/2030\n开户行：Fake Branch\n\n5500-0000-0000-0004\n\n4111111111111112\n13800138000\n";
        let result = smart_candidates(text, "粘贴");
        let cards = extra(&result, "bank_card");
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].fields["card_number"], "4111111111111111");
        assert_eq!(cards[0].fields["bank"], "Fake Bank");
        assert_eq!(cards[0].fields["holder"], "Fake Holder");
        assert_eq!(cards[0].fields["expiry"], "09/30");
        assert_eq!(cards[0].fields["branch"], "Fake Branch");
        assert_eq!(cards[0].name, "Fake Bank 尾号 1111");
        assert_eq!(cards[1].fields["card_number"], "5500000000000004");
        assert_eq!(cards[1].fields["bank"], "待补充");
        // The failing checksum and the phone number are not cards; they stay in the leftover note.
        let notes = extra(&result, "note");
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].fields["body"].contains("4111111111111112")
                && notes[0].fields["body"].contains("13800138000")
        );
        assert!(!notes[0].fields["body"].contains("4111 1111"));
    }

    #[test]
    fn unrecognised_text_is_kept_whole_as_a_secure_note() {
        let text = "meeting at noon\nbring the old hard drive\n";
        let result = smart_candidates(text, "粘贴");
        assert!(result.group.is_none());
        let notes = extra(&result, "note");
        assert_eq!(notes.len(), 1);
        assert_eq!(
            notes[0].fields["body"],
            "meeting at noon\nbring the old hard drive"
        );
        assert_eq!(notes[0].name, "粘贴的文本");
        assert!(smart_candidates("   \n\n", "粘贴").extras.is_empty());
    }

    #[test]
    fn mixed_text_splits_into_group_logins_cards_and_a_note() {
        let text = "SSH_HOST=203.0.113.7\nSSH_PASSWORD=fake-ssh\n\n账号：panel-admin\n密码：fake-panel\n\n卡号 4111111111111111\n\nremember to rotate these\n";
        let result = smart_candidates(text, "粘贴");
        assert_eq!(result.group.as_ref().unwrap().variables.len(), 2);
        assert_eq!(extra(&result, "login").len(), 1);
        assert_eq!(extra(&result, "bank_card").len(), 1);
        let notes = extra(&result, "note");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].fields["body"], "remember to rotate these");
    }

    #[test]
    fn multiline_quoted_values_stay_in_the_group() {
        let text = "SSH_PRIVATE_KEY=\"-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----\"\nSSH_HOST=h\n";
        let result = smart_candidates(text, "粘贴");
        let group = result.group.unwrap();
        assert_eq!(group.variables.len(), 2);
        assert!(group.variables[0].values[0].contains("FAKEFAKEFAKE"));
        assert!(result.extras.is_empty());
    }
}
