//! Vault use cases and policy boundary.

use secrethub_crypto::{decrypt, derive_key, encrypt, generate_salt, CryptoError, VaultKey};
use secrethub_storage::{
    CapabilityModelRow, Database, MediaAsset, NewSecret, ProviderModelRow, SecretMetadata,
    StorageError, UpdatedSecret, CAPABILITY_MODEL_ORDER,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

mod lock;
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
