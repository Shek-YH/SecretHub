//! Vault use cases and policy boundary.

use secrethub_crypto::{decrypt, derive_key, encrypt, generate_salt, CryptoError, VaultKey};
use secrethub_storage::{Database, NewSecret, SecretMetadata, StorageError, UpdatedSecret};
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
        if password.len() < 12 {
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
    pub fn get_settings(&self) -> Result<secrethub_storage::Settings, VaultError> {
        self.database.get_settings().map_err(Into::into)
    }
    pub fn update_settings(&self, settings: secrethub_storage::Settings) -> Result<(), VaultError> {
        self.database.update_settings(&settings).map_err(Into::into)
    }
    pub fn encrypted_backup(&self) -> Result<Vec<u8>, VaultError> {
        let key = self.require_key()?;
        self.database.checkpoint()?;
        let contents =
            std::fs::read(self.database.path()).map_err(|_| VaultError::InvalidSecret)?;
        secrethub_backup::create_backup(key, &contents)
            .map_err(|error| VaultError::Storage(StorageError::InvalidData(error.to_string())))
    }

    fn require_key(&self) -> Result<&VaultKey, VaultError> {
        self.key.as_ref().ok_or(VaultError::Locked)
    }
}

fn verifier_for(key: &VaultKey) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(VERIFIER_AAD);
    hash.update(key.as_ref());
    hash.finalize().to_vec()
}
