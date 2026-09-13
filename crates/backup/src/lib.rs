//! Encrypted vault backup format.

use secrethub_crypto::{decrypt, encrypt, CryptoError, EncryptedPayload, VaultKey};
use thiserror::Error;

const HEADER: &[u8] = b"SHB1";
const AAD: &[u8] = b"secrethub:backup:v1";

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("invalid backup format")]
    InvalidFormat,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

pub fn create_backup(key: &VaultKey, contents: &[u8]) -> Result<Vec<u8>, BackupError> {
    let payload = encrypt(key, contents, AAD)?;
    let mut result = HEADER.to_vec();
    result.extend_from_slice(&payload.nonce);
    result.extend_from_slice(&payload.ciphertext);
    Ok(result)
}

pub fn restore_backup(key: &VaultKey, backup: &[u8]) -> Result<Vec<u8>, BackupError> {
    if backup.len() < HEADER.len() + 12 + 16 || &backup[..HEADER.len()] != HEADER {
        return Err(BackupError::InvalidFormat);
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&backup[HEADER.len()..HEADER.len() + 12]);
    decrypt(
        key,
        &EncryptedPayload {
            nonce,
            ciphertext: backup[HEADER.len() + 12..].to_vec(),
        },
        AAD,
    )
    .map_err(Into::into)
}
