//! Vault use cases and policy boundary.

use secrethub_crypto::{decrypt, derive_key, encrypt, generate_salt, CryptoError, VaultKey};
use secrethub_storage::{
    CapabilityModelRow, Database, GroupVariableRow, ItemMigration, MediaAsset, NewSecret,
    ProviderModelRow, RekeyedPayload, SecretAttributes, SecretMetadata, StorageError,
    UpdatedSecret, CAPABILITY_MODEL_ORDER, JSON_ITEM_TYPES,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

mod batches;
pub mod groups;
pub mod items;
mod lock;
pub use batches::UndoSummary;
pub use groups::{GroupPublic, GroupUpdate, NewGroup, NewVariable, PublicVariable, VariableUpdate};
pub use items::Fields;
pub use lock::AutoLockPolicy;

const PAYLOAD_AAD: &[u8] = b"secrethub:secret-payload:v1";
const VERIFIER_AAD: &[u8] = b"secrethub:vault-verifier:v1";

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault is locked")]
    Locked,
    #[error("vault is not initialized")]
    NotInitialized,
    #[error("vault is already initialized")]
    AlreadyInitialized,
    #[error("invalid master password")]
    InvalidPassword,
    #[error("secret value is invalid")]
    InvalidSecret,
    #[error("stored secret is not valid UTF-8")]
    InvalidUtf8,
    /// Names the offending field key only, never its value.
    #[error("invalid item field: {0}")]
    InvalidField(String),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub struct NewSecretInput {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
    pub value: String,
}

/// A new login, bank card or note. `env_key` is the variable prefix of a "dev use" login.
pub struct NewItem {
    pub item_type: String,
    pub name: String,
    pub fields: Fields,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub dev_use: bool,
    pub env_key: String,
}

/// Edit of a typed item: given fields overwrite, sensitive fields left out stay unchanged and
/// `clear` removes optional fields.
pub struct ItemUpdate {
    pub name: String,
    pub fields: Fields,
    pub clear: Vec<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub dev_use: bool,
    pub env_key: String,
}

pub struct VaultService {
    database: Database,
    key: Option<VaultKey>,
}

impl VaultService {
    pub fn open(database: Database) -> Result<Self, VaultError> {
        Ok(Self {
            database,
            key: None,
        })
    }

    pub fn is_initialized(&self) -> Result<bool, VaultError> {
        Ok(self.database.get_vault_meta()?.is_some())
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    pub fn setup_master(&mut self, password: &str) -> Result<(), VaultError> {
        if self.is_initialized()? {
            return Err(VaultError::AlreadyInitialized);
        }
        if password.len() < 9 {
            return Err(VaultError::InvalidPassword);
        }
        let salt = generate_salt();
        let key = derive_key(password, &salt)?;
        let verifier = verifier_for(&key);
        self.database.set_vault_meta(&salt, &verifier)?;
        self.key = Some(key);
        // Nothing to convert in a fresh vault, but the hook keeps all entry points alike.
        let _ = self.migrate_legacy_items();
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        let Some((salt, expected_verifier)) = self.database.get_vault_meta()? else {
            return Err(VaultError::NotInitialized);
        };
        let key = derive_key(password, &salt)?;
        if verifier_for(&key)
            .as_slice()
            .ct_eq(expected_verifier.as_slice())
            .unwrap_u8()
            != 1
        {
            return Err(VaultError::InvalidPassword);
        }
        self.key = Some(key);
        // Best effort: reads fall back to the legacy interpretation if this fails.
        let _ = self.migrate_legacy_items();
        Ok(())
    }

    pub fn lock(&mut self) {
        self.key.take();
    }

    pub fn list_metadata(&self) -> Result<Vec<SecretMetadata>, VaultError> {
        self.require_key()?;
        self.database.list_metadata().map_err(Into::into)
    }

    pub fn search_metadata(&self, query: &str) -> Result<Vec<SecretMetadata>, VaultError> {
        self.require_key()?;
        self.database.search_metadata(query).map_err(Into::into)
    }

    pub fn create_secret(&self, input: NewSecretInput) -> Result<String, VaultError> {
        self.create_secret_with_attributes(input, secrethub_storage::SecretAttributes::default())
    }

    pub fn create_secret_with_attributes(
        &self,
        input: NewSecretInput,
        attributes: secrethub_storage::SecretAttributes,
    ) -> Result<String, VaultError> {
        let key = self.require_key()?;
        if input.name.trim().is_empty() || input.value.is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        let attributes = raw_string_attributes(attributes, &input.env_key)?;
        let encrypted = encrypt(key, input.value.as_bytes(), PAYLOAD_AAD)?;
        let value_hash = format!("sha256:{:x}", Sha256::digest(input.value.as_bytes()));
        let id = self.database.create_secret_with_attributes(
            NewSecret {
                name: input.name,
                provider_id: input.provider_id,
                env_key: input.env_key,
                description: input.description,
                tags: input.tags,
                ciphertext: encrypted.ciphertext,
                nonce: encrypted.nonce.to_vec(),
            },
            attributes,
        )?;
        self.database.record_audit(
            "create_secret",
            Some(&id),
            Some(&value_hash),
            "success",
            "{\"source\":\"desktop\"}",
        )?;
        Ok(id)
    }

    /// Whether an encrypted payload exists for `id`; never decrypts.
    pub fn has_secret_value(&self, id: &str) -> Result<bool, VaultError> {
        self.require_key()?;
        Ok(self.database.get_payload(id)?.is_some())
    }

    pub fn read_secret(&self, id: &str) -> Result<String, VaultError> {
        let key = self.require_key()?;
        let Some((ciphertext, nonce)) = self.database.get_payload(id)? else {
            return Err(VaultError::InvalidSecret);
        };
        if nonce.len() != 12 {
            return Err(VaultError::InvalidSecret);
        }
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&nonce);
        let plaintext = decrypt(
            key,
            &secrethub_crypto::EncryptedPayload {
                nonce: nonce_bytes,
                ciphertext,
            },
            PAYLOAD_AAD,
        )?;
        String::from_utf8(plaintext).map_err(|_| VaultError::InvalidUtf8)
    }

    pub fn update_secret(&self, id: &str, input: NewSecretInput) -> Result<(), VaultError> {
        self.update_secret_with_attributes(
            id,
            input,
            secrethub_storage::SecretAttributes::default(),
        )
    }

    pub fn update_secret_with_attributes(
        &self,
        id: &str,
        input: NewSecretInput,
        attributes: secrethub_storage::SecretAttributes,
    ) -> Result<(), VaultError> {
        let key = self.require_key()?;
        if input.name.trim().is_empty() || input.value.is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        self.reject_json_item(id)?;
        let attributes = raw_string_attributes(attributes, &input.env_key)?;
        let encrypted = encrypt(key, input.value.as_bytes(), PAYLOAD_AAD)?;
        let value_hash = format!("sha256:{:x}", Sha256::digest(input.value.as_bytes()));
        if !self.database.update_secret_with_attributes(
            id,
            UpdatedSecret {
                name: input.name,
                provider_id: input.provider_id,
                env_key: input.env_key,
                description: input.description,
                tags: input.tags,
                ciphertext: encrypted.ciphertext,
                nonce: encrypted.nonce.to_vec(),
            },
            attributes,
        )? {
            return Err(VaultError::InvalidSecret);
        }
        self.database.record_audit(
            "update_secret",
            Some(id),
            Some(&value_hash),
            "success",
            "{\"source\":\"desktop\"}",
        )?;
        Ok(())
    }

    pub fn update_metadata_with_attributes(
        &self,
        id: &str,
        input: NewSecretInput,
        attributes: secrethub_storage::SecretAttributes,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        if input.name.trim().is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        self.reject_json_item(id)?;
        let attributes = raw_string_attributes(attributes, &input.env_key)?;
        if !self.database.update_metadata_with_attributes(
            id,
            secrethub_storage::SecretMetadataUpdate {
                name: input.name,
                provider_id: input.provider_id,
                env_key: input.env_key,
                description: input.description,
                tags: input.tags,
            },
            attributes,
        )? {
            return Err(VaultError::InvalidSecret);
        }
        self.database.record_audit(
            "update_secret_metadata",
            Some(id),
            None,
            "success",
            "{\"source\":\"desktop\",\"value_preserved\":true}",
        )?;
        Ok(())
    }

    /// Typed-item payloads are JSON documents; the raw-string update paths must not touch them.
    fn reject_json_item(&self, id: &str) -> Result<(), VaultError> {
        match self.database.get_metadata(id)? {
            Some(existing) if existing.is_json_item() => Err(VaultError::InvalidSecret),
            _ => Ok(()),
        }
    }

    fn encrypt_fields(&self, fields: &Fields) -> Result<(Vec<u8>, Vec<u8>), VaultError> {
        let key = self.require_key()?;
        let encrypted = encrypt(key, items::encode_payload(fields).as_bytes(), PAYLOAD_AAD)?;
        Ok((encrypted.ciphertext, encrypted.nonce.to_vec()))
    }

    /// Fields of a stored login, bank card or note, decrypted. Callers decide what may leave.
    pub fn read_item_fields(&self, id: &str) -> Result<Fields, VaultError> {
        self.require_key()?;
        let metadata = self.json_item_metadata(id)?;
        let plaintext = self.read_secret(id)?;
        Ok(items::decode_payload(&metadata.value_type, &plaintext))
    }

    fn json_item_metadata(&self, id: &str) -> Result<SecretMetadata, VaultError> {
        match self.database.get_metadata(id)? {
            Some(metadata) if metadata.is_json_item() => Ok(metadata),
            _ => Err(VaultError::InvalidSecret),
        }
    }

    /// Creates a login, bank card or note. Returns the new id.
    pub fn create_item(&self, input: NewItem) -> Result<String, VaultError> {
        self.require_key()?;
        let def = items::json_item_type(&input.item_type)
            .ok_or_else(|| VaultError::InvalidField("item_type".into()))?;
        if input.name.trim().is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        let fields = items::validate_fields(def, &input.fields)?;
        items::check_required(def, &fields)?;
        let (dev_use, env_key) = dev_use_settings(&def.key, input.dev_use, &input.env_key)?;
        let (ciphertext, nonce) = self.encrypt_fields(&fields)?;
        let id = self.database.create_secret_with_attributes(
            NewSecret {
                name: input.name.trim().to_owned(),
                provider_id: "personal".into(),
                env_key,
                description: String::new(),
                tags: input.tags,
                ciphertext,
                nonce,
            },
            SecretAttributes {
                value_type: def.key.clone(),
                category: "personal".into(),
                favorite: input.favorite,
                subtitle: items::subtitle_for(&def.key, &fields, ""),
                dev_use,
                ..Default::default()
            },
        )?;
        self.database.record_audit(
            "create_item",
            Some(&id),
            None,
            "success",
            &format!("{{\"source\":\"desktop\",\"type\":\"{}\"}}", def.key),
        )?;
        Ok(id)
    }

    /// Edits a typed item; the type itself never changes.
    pub fn update_item(&self, id: &str, update: ItemUpdate) -> Result<(), VaultError> {
        self.require_key()?;
        let metadata = self.json_item_metadata(id)?;
        let def = items::json_item_type(&metadata.value_type).ok_or(VaultError::InvalidSecret)?;
        if update.name.trim().is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        let mut fields = self.read_item_fields(id)?;
        for (key, value) in &update.fields {
            let field = def
                .field(key)
                .ok_or_else(|| VaultError::InvalidField(key.clone()))?;
            if value.trim().is_empty() {
                // A blank sensitive field means "leave unchanged"; a blank plain field empties it.
                if !field.sensitive {
                    fields.remove(key);
                }
                continue;
            }
            fields.insert(key.clone(), items::normalize_field(field, value)?);
        }
        for key in &update.clear {
            let field = def
                .field(key)
                .ok_or_else(|| VaultError::InvalidField(key.clone()))?;
            let given = update
                .fields
                .get(key)
                .is_some_and(|value| !value.trim().is_empty());
            if field.required || given {
                return Err(VaultError::InvalidField(key.clone()));
            }
            fields.remove(key);
        }
        items::check_required(def, &fields)?;
        let (dev_use, env_key) = dev_use_settings(&def.key, update.dev_use, &update.env_key)?;
        let (ciphertext, nonce) = self.encrypt_fields(&fields)?;
        if !self.database.update_secret_with_attributes(
            id,
            UpdatedSecret {
                name: update.name.trim().to_owned(),
                provider_id: metadata.provider_id.clone(),
                env_key,
                description: String::new(),
                tags: update.tags,
                ciphertext,
                nonce,
            },
            SecretAttributes {
                value_type: def.key.clone(),
                category: "personal".into(),
                scope: metadata.scope.clone(),
                favorite: update.favorite,
                archived: metadata.archived,
                subtitle: items::subtitle_for(&def.key, &fields, ""),
                dev_use,
                ..Default::default()
            },
        )? {
            return Err(VaultError::InvalidSecret);
        }
        self.database.record_audit(
            "update_item",
            Some(id),
            None,
            "success",
            &format!("{{\"source\":\"desktop\",\"type\":\"{}\"}}", def.key),
        )?;
        Ok(())
    }

    /// Non-sensitive fields only (website, account, bank, holder, expiry, branch, notes).
    pub fn item_public_fields(&self, id: &str) -> Result<Fields, VaultError> {
        let metadata = self.json_item_metadata(id)?;
        let def = items::json_item_type(&metadata.value_type).ok_or(VaultError::InvalidSecret)?;
        let mut fields = self.read_item_fields(id)?;
        fields.retain(|key, _| def.field(key).is_some_and(|field| !field.sensitive));
        Ok(fields)
    }

    /// Names of the secret fields that currently hold a value; lets the UI show dots only for
    /// fields that exist without ever receiving a value.
    pub fn item_secret_fields(&self, id: &str) -> Result<Vec<String>, VaultError> {
        let metadata = self.json_item_metadata(id)?;
        let def = items::json_item_type(&metadata.value_type).ok_or(VaultError::InvalidSecret)?;
        Ok(self
            .read_item_fields(id)?
            .into_iter()
            .filter(|(key, value)| {
                !value.is_empty() && def.field(key).is_some_and(|field| field.sensitive)
            })
            .map(|(key, _)| key)
            .collect())
    }

    /// One field of a typed item (sensitive or not). Reveal and copy are built on this.
    pub fn item_field(&self, id: &str, field: &str) -> Result<String, VaultError> {
        let metadata = self.json_item_metadata(id)?;
        let def = items::json_item_type(&metadata.value_type).ok_or(VaultError::InvalidSecret)?;
        if def.field(field).is_none() {
            return Err(VaultError::InvalidField(field.to_owned()));
        }
        self.read_item_fields(id)?
            .remove(field)
            .filter(|value| !value.is_empty())
            .ok_or(VaultError::InvalidSecret)
    }

    fn group_metadata(&self, id: &str) -> Result<SecretMetadata, VaultError> {
        match self.database.get_metadata(id)? {
            Some(metadata) if metadata.value_type == "env_group" => Ok(metadata),
            _ => Err(VaultError::InvalidSecret),
        }
    }

    fn read_group_payload(&self, id: &str) -> Result<groups::GroupPayload, VaultError> {
        groups::GroupPayload::decode(&self.read_secret(id)?)
    }

    fn encrypt_group_payload(
        &self,
        payload: &groups::GroupPayload,
    ) -> Result<(Vec<u8>, Vec<u8>), VaultError> {
        let key = self.require_key()?;
        let encrypted = encrypt(key, payload.encode().as_bytes(), PAYLOAD_AAD)?;
        Ok((encrypted.ciphertext, encrypted.nonce.to_vec()))
    }

    /// Creates a config group. Plain values go to `group_variables`, secret values and the
    /// notes into the encrypted payload. Returns the new id.
    pub fn create_group(&self, input: NewGroup) -> Result<String, VaultError> {
        self.require_key()?;
        if input.name.trim().is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        if !groups::valid_category(&input.category) {
            return Err(VaultError::InvalidField("category".into()));
        }
        groups::check_names(
            input
                .variables
                .iter()
                .map(|variable| variable.name.as_str()),
        )?;
        groups::check_notes(&input.notes)?;
        let mut secrets = std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for variable in &input.variables {
            groups::check_value(&variable.name, &variable.value)?;
            groups::check_description(&variable.name, &variable.description)?;
            if variable.secret {
                secrets.insert(variable.name.clone(), variable.value.clone());
            }
            rows.push(GroupVariableRow {
                name: variable.name.clone(),
                secret: variable.secret,
                plain_value: if variable.secret {
                    String::new()
                } else {
                    variable.value.clone()
                },
                description: variable.description.trim().to_owned(),
            });
        }
        let (ciphertext, nonce) =
            self.encrypt_group_payload(&groups::GroupPayload::new(secrets, input.notes))?;
        let id = self.database.create_group(
            NewSecret {
                name: input.name.trim().to_owned(),
                provider_id: "group".into(),
                env_key: String::new(),
                description: String::new(),
                tags: input.tags,
                ciphertext,
                nonce,
            },
            SecretAttributes {
                value_type: "env_group".into(),
                category: input.category,
                favorite: input.favorite,
                subtitle: groups::subtitle_for(rows.len()),
                ai_values: input.ai_values,
                ..Default::default()
            },
            &rows,
        )?;
        self.database.record_audit(
            "create_group",
            Some(&id),
            None,
            "success",
            "{\"source\":\"desktop\",\"type\":\"env_group\"}",
        )?;
        Ok(id)
    }

    /// Replaces the variable list of a group. Secret variables without a value keep the stored
    /// one (looked up under `from` after a rename); plain variables must carry their value.
    pub fn update_group(&self, id: &str, update: GroupUpdate) -> Result<(), VaultError> {
        self.require_key()?;
        let metadata = self.group_metadata(id)?;
        if update.name.trim().is_empty() {
            return Err(VaultError::InvalidSecret);
        }
        if !groups::valid_category(&update.category) {
            return Err(VaultError::InvalidField("category".into()));
        }
        groups::check_names(
            update
                .variables
                .iter()
                .map(|variable| variable.name.as_str()),
        )?;
        let old_rows = self.database.list_group_variables(id)?;
        let old_payload = self.read_group_payload(id)?;
        let mut secrets = std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for variable in &update.variables {
            let source = variable.from.as_deref().unwrap_or(&variable.name);
            let kept = old_rows.iter().find(|row| row.name == source).map(|row| {
                if row.secret {
                    old_payload.secrets.get(source).cloned().unwrap_or_default()
                } else {
                    row.plain_value.clone()
                }
            });
            let value = match (&variable.value, variable.secret) {
                (Some(value), _) => value.clone(),
                (None, true) => kept.unwrap_or_default(),
                (None, false) => return Err(VaultError::InvalidField(variable.name.clone())),
            };
            groups::check_value(&variable.name, &value)?;
            let description = match &variable.description {
                Some(description) => description.trim().to_owned(),
                None => old_rows
                    .iter()
                    .find(|row| row.name == source)
                    .map(|row| row.description.clone())
                    .unwrap_or_default(),
            };
            groups::check_description(&variable.name, &description)?;
            if variable.secret {
                secrets.insert(variable.name.clone(), value.clone());
            }
            rows.push(GroupVariableRow {
                name: variable.name.clone(),
                secret: variable.secret,
                plain_value: if variable.secret {
                    String::new()
                } else {
                    value
                },
                description,
            });
        }
        let notes = match update.notes {
            Some(notes) => {
                groups::check_notes(&notes)?;
                notes
            }
            None => old_payload.notes,
        };
        let (ciphertext, nonce) =
            self.encrypt_group_payload(&groups::GroupPayload::new(secrets, notes))?;
        if !self.database.update_group(
            id,
            UpdatedSecret {
                name: update.name.trim().to_owned(),
                provider_id: metadata.provider_id.clone(),
                env_key: String::new(),
                description: String::new(),
                tags: update.tags,
                ciphertext,
                nonce,
            },
            SecretAttributes {
                value_type: "env_group".into(),
                category: update.category,
                scope: metadata.scope.clone(),
                favorite: update.favorite,
                archived: metadata.archived,
                subtitle: groups::subtitle_for(rows.len()),
                ai_values: update.ai_values,
                ..Default::default()
            },
            &rows,
        )? {
            return Err(VaultError::InvalidSecret);
        }
        self.database.record_audit(
            "update_group",
            Some(id),
            None,
            "success",
            "{\"source\":\"desktop\",\"type\":\"env_group\"}",
        )?;
        Ok(())
    }

    /// Variable names and flags of a group (no decryption).
    pub fn group_variables(&self, id: &str) -> Result<Vec<GroupVariableRow>, VaultError> {
        self.require_key()?;
        self.group_metadata(id)?;
        self.database.list_group_variables(id).map_err(Into::into)
    }

    /// What the UI may show without a reveal: plain values, names and secret flags.
    pub fn group_public(&self, id: &str) -> Result<GroupPublic, VaultError> {
        let metadata = self.group_metadata(id)?;
        let variables = self
            .group_variables(id)?
            .into_iter()
            .map(|row| PublicVariable {
                value: (!row.secret).then_some(row.plain_value),
                name: row.name,
                secret: row.secret,
                description: row.description,
            })
            .collect();
        Ok(GroupPublic {
            variables,
            has_notes: !self.read_group_payload(id)?.notes.is_empty(),
            ai_values: metadata.ai_values,
        })
    }

    /// Value of one variable, plain or secret. Reveal, copy and export are built on this.
    pub fn group_variable_value(&self, id: &str, name: &str) -> Result<String, VaultError> {
        let row = self
            .group_variables(id)?
            .into_iter()
            .find(|row| row.name == name)
            .ok_or_else(|| VaultError::InvalidField(name.to_owned()))?;
        if !row.secret {
            return Ok(row.plain_value);
        }
        Ok(self
            .read_group_payload(id)?
            .secrets
            .get(name)
            .cloned()
            .unwrap_or_default())
    }

    /// Values of the given variables (all when `names` is `None`) in display order, decrypted.
    /// Unknown names are an error. Callers decide what may leave the process.
    pub fn group_values(
        &self,
        id: &str,
        names: Option<&[String]>,
    ) -> Result<Vec<(String, String, bool)>, VaultError> {
        let rows = self.group_variables(id)?;
        if let Some(names) = names {
            if let Some(unknown) = names
                .iter()
                .find(|name| !rows.iter().any(|row| &row.name == *name))
            {
                return Err(VaultError::InvalidField(unknown.clone()));
            }
        }
        let payload = self.read_group_payload(id)?;
        Ok(rows
            .into_iter()
            .filter(|row| names.is_none_or(|names| names.contains(&row.name)))
            .map(|row| {
                let value = if row.secret {
                    payload.secrets.get(&row.name).cloned().unwrap_or_default()
                } else {
                    row.plain_value
                };
                (row.name, value, row.secret)
            })
            .collect())
    }

    /// The notes text; only for the moment the user asks to read or edit it.
    pub fn group_notes(&self, id: &str) -> Result<String, VaultError> {
        self.require_key()?;
        self.group_metadata(id)?;
        Ok(self.read_group_payload(id)?.notes)
    }

    /// Converts pre-typed-items rows (typed-items design section 7). Single transaction and
    /// idempotent: converted rows no longer match, so a second run changes nothing.
    pub fn migrate_legacy_items(&self) -> Result<usize, VaultError> {
        self.require_key()?;
        let legacy = self.database.list_legacy_items()?;
        let mut migrations = Vec::new();
        for item in legacy {
            let has_env = !item.env_key.is_empty();
            let wrapped_field = match (item.value_type.as_str(), has_env) {
                ("token", _) | (_, true) => None,
                ("password", false) => Some(("login", "password")),
                ("text", false) => Some(("note", "body")),
                ("url", false) => Some(("login", "website")),
                _ => continue,
            };
            match wrapped_field {
                None => migrations.push(ItemMigration {
                    id: item.id,
                    value_type: "dev_secret".into(),
                    category: None,
                    subtitle: item.env_key,
                    payload: None,
                }),
                Some((new_type, field)) => {
                    // An unreadable payload is left alone rather than aborting the whole run.
                    let Ok(value) = self.read_secret(&item.id) else {
                        continue;
                    };
                    let fields = Fields::from([(field.to_owned(), value)]);
                    let (ciphertext, nonce) = self.encrypt_fields(&fields)?;
                    migrations.push(ItemMigration {
                        id: item.id,
                        value_type: new_type.into(),
                        category: Some("personal".into()),
                        subtitle: items::subtitle_for(new_type, &fields, ""),
                        payload: Some((ciphertext, nonce)),
                    });
                }
            }
        }
        if migrations.is_empty() {
            return Ok(0);
        }
        self.database.apply_item_migrations(&migrations)?;
        self.database.record_audit(
            "migrate_legacy_items",
            None,
            None,
            "success",
            &format!("{{\"source\":\"desktop\",\"count\":{}}}", migrations.len()),
        )?;
        Ok(migrations.len())
    }

    /// Re-encrypts the whole vault under a new master password. Everything is prepared in
    /// memory and swapped in one transaction, so a failure leaves the old password valid.
    pub fn change_master_password(
        &mut self,
        current: &str,
        new_password: &str,
    ) -> Result<(), VaultError> {
        let old_key = self.require_key()?;
        let Some((salt, expected_verifier)) = self.database.get_vault_meta()? else {
            return Err(VaultError::NotInitialized);
        };
        let derived = derive_key(current, &salt)?;
        if verifier_for(&derived)
            .as_slice()
            .ct_eq(expected_verifier.as_slice())
            .unwrap_u8()
            != 1
        {
            return Err(VaultError::InvalidPassword);
        }
        if new_password.len() < 9 {
            return Err(VaultError::InvalidPassword);
        }
        let new_salt = generate_salt();
        let new_key = derive_key(new_password, &new_salt)?;
        let mut rekeyed = Vec::new();
        for (secret_id, ciphertext, nonce) in self.database.list_payloads()? {
            let nonce: [u8; 12] = nonce
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::InvalidSecret)?;
            let plaintext = decrypt(
                old_key,
                &secrethub_crypto::EncryptedPayload { nonce, ciphertext },
                PAYLOAD_AAD,
            )?;
            let encrypted = encrypt(&new_key, &plaintext, PAYLOAD_AAD)?;
            rekeyed.push(RekeyedPayload {
                secret_id,
                ciphertext: encrypted.ciphertext,
                nonce: encrypted.nonce.to_vec(),
            });
        }
        self.database
            .rekey(&new_salt, &verifier_for(&new_key), &rekeyed)?;
        self.key = Some(new_key);
        // Both are best effort: the new password is already the only valid one.
        let _ = self.database.checkpoint();
        let _ = self.database.record_audit(
            "master_password_changed",
            None,
            None,
            "success",
            "{\"source\":\"desktop\"}",
        );
        Ok(())
    }

    pub fn delete_secret(&self, id: &str) -> Result<bool, VaultError> {
        self.require_key()?;
        let deleted = self.database.delete_secret(id)?;
        if deleted {
            self.database.record_audit(
                "delete_secret",
                Some(id),
                None,
                "success",
                "{\"source\":\"desktop\"}",
            )?;
        }
        Ok(deleted)
    }

    pub fn list_audit(&self) -> Result<Vec<secrethub_storage::AuditEvent>, VaultError> {
        self.database.list_audit().map_err(Into::into)
    }
    pub fn record_action(
        &self,
        operation: &str,
        secret_id: Option<&str>,
        metadata_json: &str,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        self.database
            .record_audit(operation, secret_id, None, "success", metadata_json)
            .map_err(Into::into)
    }
    /// Replaces a key's synced model list and records the sync outcome in one transaction.
    pub fn replace_provider_models(
        &self,
        secret_id: &str,
        rows: &[ProviderModelRow],
        synced_at: i64,
        status: &str,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        self.database
            .replace_provider_models(secret_id, rows, synced_at, status)
            .map_err(Into::into)
    }

    /// Records a failed/unsupported sync; stored model rows are kept.
    pub fn set_models_sync_status(
        &self,
        secret_id: &str,
        status: &str,
        at: i64,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        if !self
            .database
            .set_models_sync_status(secret_id, status, at)?
        {
            return Err(VaultError::InvalidSecret);
        }
        Ok(())
    }

    pub fn list_provider_models(
        &self,
        secret_id: &str,
    ) -> Result<Vec<ProviderModelRow>, VaultError> {
        self.require_key()?;
        self.database
            .list_provider_models(secret_id)
            .map_err(Into::into)
    }

    // Media assets are not secrets: these work while the vault is locked.
    pub fn insert_media(&self, asset: &MediaAsset) -> Result<(), VaultError> {
        self.database.insert_media(asset).map_err(Into::into)
    }
    pub fn get_media(&self, id: &str) -> Result<Option<MediaAsset>, VaultError> {
        self.database.get_media(id).map_err(Into::into)
    }
    pub fn find_media_by_sha(&self, sha256: &str) -> Result<Option<MediaAsset>, VaultError> {
        self.database.find_media_by_sha(sha256).map_err(Into::into)
    }
    pub fn list_media(&self) -> Result<Vec<MediaAsset>, VaultError> {
        self.database.list_media().map_err(Into::into)
    }
    pub fn update_media_meta(
        &self,
        id: &str,
        description: &str,
        tags: &[String],
    ) -> Result<bool, VaultError> {
        self.database
            .update_media_meta(id, description, tags)
            .map_err(Into::into)
    }
    pub fn delete_media(&self, id: &str) -> Result<Option<MediaAsset>, VaultError> {
        self.database.delete_media(id).map_err(Into::into)
    }

    /// A key's per-capability model choices (everything except the chat model).
    pub fn list_capability_models(
        &self,
        secret_id: &str,
    ) -> Result<Vec<CapabilityModelRow>, VaultError> {
        self.require_key()?;
        self.database
            .list_capability_models(secret_id)
            .map_err(Into::into)
    }

    /// Replaces a key's capability models. Rows with an empty model id are dropped; env names
    /// must be valid, unique within the key and distinct from the key's own env names.
    pub fn set_capability_models(
        &self,
        secret_id: &str,
        rows: &[CapabilityModelRow],
    ) -> Result<(), VaultError> {
        self.require_key()?;
        let Some(metadata) = self.database.get_metadata(secret_id)? else {
            return Err(VaultError::InvalidSecret);
        };
        let mut used: Vec<&str> = [
            metadata.env_key.as_str(),
            metadata.model_env_key.as_str(),
            metadata.endpoint_env_key.as_str(),
        ]
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
        let mut seen_capabilities = Vec::new();
        let mut kept = Vec::new();
        for row in rows.iter().filter(|row| !row.model_id.trim().is_empty()) {
            if !CAPABILITY_MODEL_ORDER.contains(&row.capability.as_str())
                || seen_capabilities.contains(&row.capability.as_str())
                || row.model_id.chars().count() > 200
                || row.model_id.chars().any(char::is_control)
            {
                return Err(VaultError::InvalidSecret);
            }
            seen_capabilities.push(row.capability.as_str());
            let env_key = row.env_key.trim();
            if !env_key.is_empty() {
                if !valid_env_name(env_key) || used.contains(&env_key) {
                    return Err(VaultError::InvalidSecret);
                }
                used.push(env_key);
            }
            kept.push(CapabilityModelRow {
                capability: row.capability.clone(),
                model_id: row.model_id.trim().to_owned(),
                env_key: env_key.to_owned(),
            });
        }
        self.database.replace_capability_models(secret_id, &kept)?;
        self.database.record_audit(
            "capability_models_set",
            Some(secret_id),
            None,
            "success",
            &format!("{{\"source\":\"desktop\",\"count\":{}}}", kept.len()),
        )?;
        Ok(())
    }

    /// Stores a user-confirmed capability override for (provider, model). Metadata only.
    pub fn set_capability_override(
        &self,
        provider_id: &str,
        model_id: &str,
        capabilities: &[String],
        source: &str,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        if provider_id.is_empty()
            || model_id.is_empty()
            || capabilities.is_empty()
            || !matches!(source, "user" | "ai")
        {
            return Err(VaultError::InvalidSecret);
        }
        self.database
            .upsert_capability_override(provider_id, model_id, capabilities, source)
            .map_err(Into::into)
    }

    pub fn update_validation_status(&self, id: &str, status: &str) -> Result<(), VaultError> {
        self.require_key()?;
        if !self.database.update_status(id, status)? {
            return Err(VaultError::InvalidSecret);
        }
        self.database.record_audit(
            "validate_secret",
            Some(id),
            None,
            "success",
            &format!("{{\"source\":\"desktop\",\"status\":\"{status}\"}}"),
        )?;
        Ok(())
    }
    pub fn save_profile(
        &self,
        profile: secrethub_storage::StoredProfile,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        self.database.save_profile(&profile).map_err(Into::into)
    }
    pub fn list_profiles(&self) -> Result<Vec<secrethub_storage::StoredProfile>, VaultError> {
        self.require_key()?;
        self.database.list_profiles().map_err(Into::into)
    }
    pub fn delete_profile(&self, id: &str) -> Result<bool, VaultError> {
        self.require_key()?;
        self.database.delete_profile(id).map_err(Into::into)
    }
    pub fn record_project(
        &self,
        path: &str,
        display_name: &str,
    ) -> Result<secrethub_storage::ProjectRecord, VaultError> {
        self.require_key()?;
        self.database
            .record_project(path, display_name)
            .map_err(Into::into)
    }
    pub fn list_projects(&self) -> Result<Vec<secrethub_storage::ProjectRecord>, VaultError> {
        self.require_key()?;
        self.database.list_projects().map_err(Into::into)
    }
    pub fn list_tags(&self) -> Result<Vec<String>, VaultError> {
        self.require_key()?;
        self.database.list_tags().map_err(Into::into)
    }
    pub fn create_tag(&self, name: &str) -> Result<String, VaultError> {
        self.require_key()?;
        self.database.create_tag(name).map_err(Into::into)
    }
    pub fn delete_tag(&self, name: &str) -> Result<bool, VaultError> {
        self.require_key()?;
        self.database.delete_tag(name).map_err(Into::into)
    }
    pub fn get_settings(&self) -> Result<secrethub_storage::Settings, VaultError> {
        self.database.get_settings().map_err(Into::into)
    }
    pub fn update_settings(&self, settings: secrethub_storage::Settings) -> Result<(), VaultError> {
        self.database.update_settings(&settings).map_err(Into::into)
    }
    pub fn catalog_auto_update(&self) -> Result<bool, VaultError> {
        self.database.catalog_auto_update().map_err(Into::into)
    }
    pub fn set_catalog_auto_update(&self, enabled: bool) -> Result<(), VaultError> {
        self.database
            .set_catalog_auto_update(enabled)
            .map_err(Into::into)
    }
    /// Whole database encrypted as an `SHB2` backup that embeds the vault salt, so the master
    /// password alone is enough to restore it on any machine.
    pub fn encrypted_backup(&self) -> Result<Vec<u8>, VaultError> {
        let key = self.require_key()?;
        let Some((salt, _)) = self.database.get_vault_meta()? else {
            return Err(VaultError::NotInitialized);
        };
        self.database.checkpoint()?;
        let contents =
            std::fs::read(self.database.path()).map_err(|_| VaultError::InvalidSecret)?;
        secrethub_backup::create_backup_v2(key, &salt, &contents)
            .map_err(|error| VaultError::Storage(StorageError::InvalidData(error.to_string())))
    }

    /// Decrypts an old `SHB1` backup with this unlocked vault's own key. Fails for backups of
    /// any other vault, because the key is not stored in the file.
    pub fn decrypt_legacy_backup(&self, backup: &[u8]) -> Result<Vec<u8>, VaultError> {
        let key = self.require_key()?;
        secrethub_backup::restore_backup(key, backup).map_err(|_| VaultError::InvalidPassword)
    }

    fn require_key(&self) -> Result<&VaultKey, VaultError> {
        self.key.as_ref().ok_or(VaultError::Locked)
    }
}

/// Raw-string items only: typed JSON items are created through `create_item`. Developer
/// secrets show their variable name as the list subtitle.
fn raw_string_attributes(
    mut attributes: SecretAttributes,
    env_key: &str,
) -> Result<SecretAttributes, VaultError> {
    if JSON_ITEM_TYPES.contains(&attributes.value_type.as_str()) {
        return Err(VaultError::InvalidSecret);
    }
    attributes.dev_use = false;
    attributes.subtitle = if attributes.value_type == "dev_secret" {
        env_key.to_owned()
    } else {
        String::new()
    };
    Ok(attributes)
}

/// "Dev use" exists for logins only and needs a valid variable prefix; otherwise it is off
/// and the prefix is cleared.
fn dev_use_settings(
    item_type: &str,
    dev_use: bool,
    env_key: &str,
) -> Result<(bool, String), VaultError> {
    if item_type != "login" || !dev_use {
        return Ok((false, String::new()));
    }
    let prefix = env_key.trim();
    if prefix.len() > 100 || !valid_env_name(prefix) {
        return Err(VaultError::InvalidField("env_key".into()));
    }
    Ok((true, prefix.to_owned()))
}

fn valid_env_name(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some('_' | 'A'..='Z'))
        && characters.all(|character| {
            character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit()
        })
}

fn verifier_for(key: &VaultKey) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(VERIFIER_AAD);
    hash.update(key.as_ref());
    hash.finalize().to_vec()
}
