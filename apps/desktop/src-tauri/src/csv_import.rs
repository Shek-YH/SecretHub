//! CSV sources of the import assistant (design section 3.4): password exports of browsers and
//! password managers become login candidates, and the bank card template becomes bank card
//! candidates. A CSV with another header is not recognised here and is kept as a note by the
//! caller. Values are carried in memory only; skipped rows are reported by number, never by
//! content.

use crate::scan::{Extra, SkippedFile};
use secrethub_core::items;
use std::collections::BTreeMap;

/// Header of the downloadable bank card template. It has no CVV and no PIN column on purpose.
pub const BANK_TEMPLATE_HEADER: &str = "名称,银行,卡号,持卡人,有效期,开户行,备注";

/// The template file: UTF-8 with a byte order mark so spreadsheet programs show the Chinese header.
pub fn bank_template() -> String {
    format!("\u{feff}{BANK_TEMPLATE_HEADER}\r\n")
}

/// RFC 4180 parsing: quoted fields, doubled quotes, line breaks inside quotes.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = text.chars().peekable();
    while let Some(ch) = characters.next() {
        if quoted {
            match ch {
                '"' if characters.peek() == Some(&'"') => {
                    field.push('"');
                    characters.next();
                }
                '"' => quoted = false,
                other => field.push(other),
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            other => field.push(other),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|row| row.iter().any(|cell| !cell.trim().is_empty()));
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `name,url,username,password` (Chrome, Edge) and the Firefox `url,username,password,...`.
    Browser,
    /// `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp`
    Bitwarden,
    BankTemplate,
}

fn normalize_header(cell: &str) -> String {
    cell.trim().to_lowercase()
}

pub fn detect(header: &[String]) -> Option<Format> {
    let names: Vec<String> = header.iter().map(|cell| normalize_header(cell)).collect();
    let has = |name: &str| names.iter().any(|cell| cell == name);
    if has("login_uri") && has("login_username") && has("login_password") {
        return Some(Format::Bitwarden);
    }
    if has("url") && has("username") && has("password") {
        return Some(Format::Browser);
    }
    if (has("卡号") && has("银行")) || (has("card_number") && has("bank")) {
        return Some(Format::BankTemplate);
    }
    None
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|cell| names.contains(&normalize_header(cell).as_str()))
}

fn cell(row: &[String], index: Option<usize>) -> String {
    index
        .and_then(|index| row.get(index))
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// `otpauth://totp/...?secret=ABC` or a bare Base32 secret.
fn totp_secret(raw: &str) -> String {
    let raw = raw.trim();
    if let Some((_, query)) = raw.split_once('?') {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("secret=") {
                return value.to_owned();
            }
        }
    }
    raw.to_owned()
}

fn valid_fields(
    item_type: &str,
    fields: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let def = items::json_item_type(item_type)?;
    let validated = items::validate_fields(def, fields).ok()?;
    items::check_required(def, &validated).ok()?;
    Some(validated)
}

fn login_extra(source: &str, name: &str, fields: BTreeMap<String, String>) -> Option<Extra> {
    // A website or TOTP value that does not validate is dropped instead of losing the whole row.
    let mut fields = fields;
    for key in ["website", "totp"] {
        if let Some(value) = fields.get(key).cloned() {
            let valid = items::json_item_type("login")
                .and_then(|def| def.field(key))
                .is_some_and(|def| items::normalize_field(def, &value).is_ok());
            if !valid {
                fields.remove(key);
            }
        }
    }
    let fields = valid_fields("login", &fields)?;
    let host = fields.get("website").and_then(|site| items::host_of(site));
    let name = if name.trim().is_empty() {
        host.clone()
            .or_else(|| fields.get("username").cloned())
            .unwrap_or_default()
    } else {
        name.trim().to_owned()
    };
    Some(Extra {
        source: source.to_owned(),
        kind: "login",
        name,
        fields,
    })
}

/// `MM/YY`, or the common `MM/YYYY` and `YYYY-MM` spellings converted to it; `None` otherwise.
pub(crate) fn normalize_expiry(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let digits_only = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let candidate = if let Some((left, right)) = raw.split_once('/') {
        let (month, year) = (left.trim(), right.trim());
        if digits_only(month) && digits_only(year) {
            let year = if year.len() == 4 { &year[2..] } else { year };
            Some(format!("{:0>2}/{}", month, year))
        } else {
            None
        }
    } else if let Some((year, month)) = raw.split_once('-') {
        if digits_only(year) && digits_only(month) && year.len() == 4 {
            Some(format!("{:0>2}/{}", month, &year[2..]))
        } else {
            None
        }
    } else {
        None
    };
    candidate.filter(|value| {
        items::json_item_type("bank_card")
            .and_then(|def| def.field("expiry"))
            .is_some_and(|def| items::normalize_field(def, value).is_ok())
    })
}

fn card_extra(source: &str, name: &str, mut fields: BTreeMap<String, String>) -> Option<Extra> {
    if let Some(expiry) = fields.remove("expiry") {
        if let Some(valid) = normalize_expiry(&expiry) {
            fields.insert("expiry".to_owned(), valid);
        }
    }
    let fields = valid_fields("bank_card", &fields)?;
    let number = fields.get("card_number")?;
    let last4: String = number
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let name = if name.trim().is_empty() {
        format!(
            "{} 尾号 {last4}",
            fields.get("bank").cloned().unwrap_or_default()
        )
    } else {
        name.trim().to_owned()
    };
    Some(Extra {
        source: source.to_owned(),
        kind: "bank_card",
        name,
        fields,
    })
}

pub struct CsvOutcome {
    pub extras: Vec<Extra>,
    pub skipped: Vec<SkippedFile>,
}

/// Recognises a browser, Bitwarden or bank template CSV. `None` for anything else.
pub fn extras_from_csv(file_name: &str, text: &str, source: &str) -> Option<CsvOutcome> {
    let rows = parse_csv(text);
    let (header, data) = rows.split_first()?;
    let format = detect(header)?;
    let mut extras = Vec::new();
    let mut skipped = 0usize;
    for row in data {
        let candidate = match format {
            Format::Browser => {
                let (name, url, user, pass, note) = (
                    cell(row, column(header, &["name", "title"])),
                    cell(row, column(header, &["url"])),
                    cell(row, column(header, &["username"])),
                    cell(row, column(header, &["password"])),
                    cell(row, column(header, &["note", "notes"])),
                );
                let mut fields = BTreeMap::new();
                for (key, value) in [
                    ("website", url),
                    ("username", user),
                    ("password", pass),
                    ("notes", note),
                ] {
                    if !value.is_empty() {
                        fields.insert(key.to_owned(), value);
                    }
                }
                login_extra(source, &name, fields)
            }
            Format::Bitwarden => {
                let kind = cell(row, column(header, &["type"]));
                if !kind.is_empty() && kind != "login" {
                    continue;
                }
                let mut fields = BTreeMap::new();
                for (key, value) in [
                    ("website", cell(row, column(header, &["login_uri"]))),
                    ("username", cell(row, column(header, &["login_username"]))),
                    ("password", cell(row, column(header, &["login_password"]))),
                    (
                        "totp",
                        totp_secret(&cell(row, column(header, &["login_totp"]))),
                    ),
                    ("notes", cell(row, column(header, &["notes"]))),
                ] {
                    if !value.is_empty() {
                        fields.insert(key.to_owned(), value);
                    }
                }
                login_extra(source, &cell(row, column(header, &["name"])), fields)
            }
            Format::BankTemplate => {
                let mut fields = BTreeMap::new();
                for (key, names) in [
                    ("bank", &["银行", "bank"][..]),
                    ("card_number", &["卡号", "card_number"][..]),
                    ("holder", &["持卡人", "holder"][..]),
                    ("expiry", &["有效期", "expiry"][..]),
                    ("branch", &["开户行", "branch"][..]),
                    ("notes", &["备注", "notes"][..]),
                ] {
                    let value = cell(row, column(header, names));
                    if !value.is_empty() {
                        fields.insert(key.to_owned(), value);
                    }
                }
                card_extra(
                    source,
                    &cell(row, column(header, &["名称", "name"])),
                    fields,
                )
            }
        };
        match candidate {
            Some(extra) => extras.push(extra),
            None => skipped += 1,
        }
    }
    let skipped = (0..skipped)
        .map(|_| SkippedFile {
            name: file_name.to_owned(),
            reason: "invalid_row",
        })
        .collect();
    Some(CsvOutcome { extras, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_card() -> &'static str {
        "4111111111111111"
    }

    #[test]
    fn parses_quotes_commas_newlines_and_the_bom() {
        let rows = parse_csv(
            "\u{feff}a,b,c\r\n\"x,1\",\"he said \"\"hi\"\"\",\"line1\nline2\"\n\n,,\nlast,row,end",
        );
        assert_eq!(rows[0], ["a", "b", "c"]);
        assert_eq!(rows[1], ["x,1", "he said \"hi\"", "line1\nline2"]);
        assert_eq!(rows[2], ["last", "row", "end"]);
        assert_eq!(rows.len(), 3, "blank lines and all-empty rows are dropped");
    }

    #[test]
    fn detects_the_three_documented_headers() {
        let header = |text: &str| text.split(',').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            detect(&header("name,url,username,password")),
            Some(Format::Browser)
        );
        assert_eq!(
            detect(&header("name,url,username,password,note")),
            Some(Format::Browser)
        );
        assert_eq!(
            detect(&header(
                "url,username,password,httpRealm,formActionOrigin,guid,timeCreated"
            )),
            Some(Format::Browser)
        );
        assert_eq!(
            detect(&header("folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp")),
            Some(Format::Bitwarden)
        );
        assert_eq!(
            detect(&header(BANK_TEMPLATE_HEADER)),
            Some(Format::BankTemplate)
        );
        assert_eq!(detect(&header("name,team")), None);
        assert_eq!(detect(&header("url,username")), None);
    }

    #[test]
    fn chrome_and_edge_rows_become_logins() {
        let csv = "name,url,username,password,note\n\
             Example,https://accounts.example.com/login,fake-user@example.com,fake-pass-1,\"two\nlines\"\n\
             ,https://other.example.org,second-user,fake-pass-2,\n\
             Broken,not a url,third,fake-pass-3,\n\
             NoUser,https://x.example.com,,fake-pass-4,\n";
        let outcome = extras_from_csv("passwords.csv", csv, "folder").unwrap();
        let names: Vec<_> = outcome.extras.iter().map(|e| e.name.as_str()).collect();
        // The second row has no title, so its host names it; the invalid website is dropped
        // but the row stays; the row without a user name is skipped by number only.
        assert_eq!(names, ["Example", "other.example.org", "Broken"]);
        assert_eq!(outcome.extras[0].fields["notes"], "two\nlines");
        assert_eq!(
            outcome.extras[0].fields["website"],
            "https://accounts.example.com/login"
        );
        assert!(!outcome.extras[2].fields.contains_key("website"));
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].reason, "invalid_row");
        assert!(outcome.extras.iter().all(|e| e.kind == "login"));
    }

    #[test]
    fn firefox_rows_use_the_host_as_the_name() {
        let csv = "url,username,password,httpRealm,formActionOrigin,guid\nhttps://www.example.com,ff-user,ff-pass,,https://www.example.com,{guid}\n";
        let outcome = extras_from_csv("logins.csv", csv, "s").unwrap();
        assert_eq!(outcome.extras.len(), 1);
        assert_eq!(outcome.extras[0].name, "www.example.com");
        assert_eq!(outcome.extras[0].fields["username"], "ff-user");
    }

    #[test]
    fn bitwarden_rows_keep_the_totp_secret_and_skip_other_item_types() {
        let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                   Work,1,login,Fake Site,some note,,0,https://login.example.com,bw-user,bw-pass,jbsw y3dp ehpk 3pxp\n\
                   ,,login,Via URI,,,0,https://uri.example.com,uri-user,uri-pass,otpauth://totp/Example:uri-user?secret=JBSWY3DPEHPK3PXP&issuer=Example\n\
                   ,,note,A secure note,body text,,0,,,,\n\
                   ,,login,Bad totp,,,0,https://bad.example.com,bad-user,bad-pass,!!!\n";
        let outcome = extras_from_csv("bitwarden_export.csv", csv, "s").unwrap();
        let names: Vec<_> = outcome.extras.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Fake Site", "Via URI", "Bad totp"]);
        assert_eq!(outcome.extras[0].fields["totp"], "JBSWY3DPEHPK3PXP");
        assert_eq!(outcome.extras[1].fields["totp"], "JBSWY3DPEHPK3PXP");
        assert_eq!(outcome.extras[0].fields["notes"], "some note");
        assert!(!outcome.extras[2].fields.contains_key("totp"));
        assert!(
            outcome.skipped.is_empty(),
            "other item types are ignored, not counted as bad rows"
        );
    }

    #[test]
    fn the_bank_template_has_no_secret_columns_and_round_trips() {
        let template = bank_template();
        assert!(
            template.starts_with('\u{feff}')
                && template.contains("名称,银行,卡号,持卡人,有效期,开户行,备注")
        );
        for forbidden in ["CVV", "cvv", "密码", "PIN", "pin"] {
            assert!(!template.contains(forbidden), "{forbidden}");
        }
        // Header only: parsing it yields no cards.
        let empty = extras_from_csv("bank-cards.csv", &template, "s").unwrap();
        assert!(empty.extras.is_empty() && empty.skipped.is_empty());

        let filled = format!(
            "{template}工资卡,Fake Bank,{}  ,Fake Holder,09/2030,Fake Branch,\"note, with comma\"\r\n,Other Bank,4111 1111 1111 1111,,2031-02,,\r\nBad,Fake Bank,1234,,,,\r\n",
            "4111-1111-1111-1111"
        );
        let outcome = extras_from_csv("bank-cards.csv", &filled, "s").unwrap();
        assert_eq!(outcome.extras.len(), 2);
        let first = &outcome.extras[0];
        assert_eq!((first.kind, first.name.as_str()), ("bank_card", "工资卡"));
        assert_eq!(first.fields["card_number"], fake_card());
        assert_eq!(first.fields["expiry"], "09/30");
        assert_eq!(first.fields["notes"], "note, with comma");
        assert!(!first.fields.contains_key("cvv") && !first.fields.contains_key("pin"));
        let second = &outcome.extras[1];
        assert_eq!(second.name, "Other Bank 尾号 1111");
        assert_eq!(second.fields["expiry"], "02/31");
        assert_eq!(outcome.skipped.len(), 1, "a short card number is a bad row");
    }

    #[test]
    fn unknown_headers_are_not_claimed() {
        assert!(extras_from_csv("users.csv", "name,team\nfake,ops\n", "s").is_none());
        assert!(extras_from_csv("empty.csv", "", "s").is_none());
    }

    #[test]
    fn expiry_spellings_normalize_or_drop() {
        assert_eq!(normalize_expiry("09/30").as_deref(), Some("09/30"));
        assert_eq!(normalize_expiry("9/2030").as_deref(), Some("09/30"));
        assert_eq!(normalize_expiry("2030-09").as_deref(), Some("09/30"));
        for bad in ["", "13/30", "09-30", "soon"] {
            assert_eq!(normalize_expiry(bad), None, "{bad}");
        }
    }
}
