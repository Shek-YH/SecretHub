//! Commands for typed items (login, bank card, note). Secret fields only ever leave through
//! a native dialog or the clipboard; the note body is the one documented exception.

use super::commands::{copy_to_clipboard, touch_activity, vault_from_state};
use super::AppState;
use secrethub_core::{items, Fields, ItemUpdate, NewItem, VaultService};
use serde::Deserialize;
use tauri::State;

const CLIPBOARD_SECONDS: u64 = 30;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCreateRequest {
    pub item_type: String,
    pub name: String,
    #[serde(default)]
    pub fields: Fields,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub dev_use: Option<bool>,
    #[serde(default)]
    pub env_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemUpdateRequest {
    pub name: String,
    #[serde(default)]
    pub fields: Fields,
    #[serde(default)]
    pub clear: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub dev_use: Option<bool>,
    #[serde(default)]
    pub env_key: Option<String>,
}

fn vault_error(error: secrethub_core::VaultError) -> String {
    error.to_string()
}

/// Item type of `id`, or an error when it is not a typed item.
fn typed_item_type(vault: &VaultService, id: &str) -> Result<String, String> {
    vault
        .list_metadata()
        .map_err(vault_error)?
        .into_iter()
        .find(|item| item.id == id && item.is_json_item())
        .map(|item| item.value_type)
        .ok_or_else(|| "item not found".to_owned())
}

fn audit(vault: &VaultService, operation: &str, id: &str, item_type: &str) -> Result<(), String> {
    vault
        .record_action(
            operation,
            Some(id),
            &format!("{{\"source\":\"desktop\",\"type\":\"{item_type}\"}}"),
        )
        .map_err(vault_error)
}

/// Text shown in the native dialog; card numbers are grouped by four for readability.
fn reveal_text(field_kind: &str, value: &str) -> String {
    if field_kind != "card_number" {
        return value.to_owned();
    }
    value
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
pub fn item_create(
    request: ItemCreateRequest,
    state: State<'_, AppState>,
) -> Result<String, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .create_item(NewItem {
            item_type: request.item_type,
            name: request.name,
            fields: request.fields,
            tags: request.tags,
            favorite: request.favorite,
            dev_use: request.dev_use.unwrap_or(false),
            env_key: request.env_key.unwrap_or_default(),
        })
        .map_err(vault_error)
}

#[tauri::command]
pub fn item_update(
    id: String,
    request: ItemUpdateRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .update_item(
            &id,
            ItemUpdate {
                name: request.name,
                fields: request.fields,
                clear: request.clear,
                tags: request.tags,
                favorite: request.favorite,
                dev_use: request.dev_use.unwrap_or(false),
                env_key: request.env_key.unwrap_or_default(),
            },
        )
        .map_err(vault_error)
}

/// Non-secret fields for the detail page and for pre-filling the edit form.
#[tauri::command]
pub fn item_public_fields(id: String, state: State<'_, AppState>) -> Result<Fields, String> {
    touch_activity(&state)?;
    vault_from_state(&state)?
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?
        .item_public_fields(&id)
        .map_err(vault_error)
}

/// Shows one secret field in a native dialog; the value never reaches the web view.
#[tauri::command]
pub fn item_field_reveal(
    id: String,
    field: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    let (value, item_type, kind) = {
        let guard = vault_from_state(&state)?;
        let vault = guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        let item_type = typed_item_type(vault, &id)?;
        let kind = items::json_item_type(&item_type)
            .and_then(|def| def.field(&field))
            .filter(|def| def.sensitive)
            .map(|def| def.kind.clone())
            .ok_or_else(|| "field is not a secret field".to_owned())?;
        (
            vault.item_field(&id, &field).map_err(vault_error)?,
            item_type,
            kind,
        )
    };
    rfd::MessageDialog::new()
        .set_title("SecretHub")
        .set_description(reveal_text(&kind, &value))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    let guard = vault_from_state(&state)?;
    audit(
        guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?,
        "item_field_revealed",
        &id,
        &item_type,
    )
}

/// Copies one field with the usual auto-clearing clipboard; the web view never sees it.
#[tauri::command]
pub fn item_field_copy(
    id: String,
    field: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_activity(&state)?;
    let (value, item_type) = {
        let guard = vault_from_state(&state)?;
        let vault = guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?;
        let item_type = typed_item_type(vault, &id)?;
        (
            vault.item_field(&id, &field).map_err(vault_error)?,
            item_type,
        )
    };
    copy_to_clipboard(value, CLIPBOARD_SECONDS)?;
    let guard = vault_from_state(&state)?;
    audit(
        guard
            .as_ref()
            .ok_or_else(|| "vault unavailable".to_owned())?,
        "item_field_copied",
        &id,
        &item_type,
    )
}

/// The one exception to "secrets stay out of the web view": long note text is returned to
/// the UI, only when the user asks to read or edit it.
#[tauri::command]
pub fn item_note_body(id: String, state: State<'_, AppState>) -> Result<String, String> {
    touch_activity(&state)?;
    let guard = vault_from_state(&state)?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| "vault unavailable".to_owned())?;
    if typed_item_type(vault, &id)? != "note" {
        return Err("item is not a note".to_owned());
    }
    let body = vault.item_field(&id, "body").map_err(vault_error)?;
    audit(vault, "item_note_body_read", &id, "note")?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_camel_case_names() {
        let request: ItemCreateRequest = serde_json::from_str(
            r#"{"itemType":"login","name":"Fake","fields":{"username":"u"},"devUse":true,"envKey":"FAKE_TEST","tags":["a"],"favorite":true}"#,
        )
        .unwrap();
        assert_eq!(request.item_type, "login");
        assert_eq!(request.dev_use, Some(true));
        assert_eq!(request.env_key.as_deref(), Some("FAKE_TEST"));
        let update: ItemUpdateRequest =
            serde_json::from_str(r#"{"name":"Fake","clear":["totp"]}"#).unwrap();
        assert_eq!(update.clear, ["totp"]);
        assert!(update.fields.is_empty() && update.dev_use.is_none());
    }

    #[test]
    fn card_numbers_are_grouped_for_the_dialog_only() {
        assert_eq!(
            reveal_text("card_number", "4111111111111111"),
            "4111 1111 1111 1111"
        );
        assert_eq!(
            reveal_text("card_number", "378282246310005"),
            "3782 8224 6310 005"
        );
        assert_eq!(reveal_text("secret", "fake-password"), "fake-password");
    }
}
