//! Encrypted vault backup formats.
//!
//! - `SHB1`: magic, nonce, ciphertext. The key comes from the vault that wrote it, so the
//!   file can only be restored by that same (already unlocked) vault.
//! - `SHB2`: magic, format version, salt length, salt, nonce, ciphertext. The salt travels
//!   with the file, so the vault master password alone can derive the key anywhere.
//!   Format version 1 means Argon2 with the crate defaults (`secrethub_crypto::derive_key`).

use secrethub_crypto::{decrypt, derive_key, encrypt, CryptoError, EncryptedPayload, VaultKey};
use thiserror::Error;

const HEADER: &[u8] = b"SHB1";
const AAD: &[u8] = b"secrethub:backup:v1";
const HEADER_V2: &[u8] = b"SHB2";
const AAD_V2: &[u8] = b"secrethub:backup:v2";
const FORMAT_VERSION_V2: u8 = 1;
const MIN_SALT_LEN: usize = 8;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("invalid backup format")]
    InvalidFormat,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupFormat {
    V1,
    V2,
}

/// Looks at the magic bytes only; nothing is decrypted.
pub fn detect_format(backup: &[u8]) -> Option<BackupFormat> {
    if backup.starts_with(HEADER_V2) {
        Some(BackupFormat::V2)
    } else if backup.starts_with(HEADER) {
        Some(BackupFormat::V1)
    } else {
        None
    }
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

/// Writes an `SHB2` backup. `salt` must be the vault salt that `key` was derived with.
pub fn create_backup_v2(
    key: &VaultKey,
    salt: &[u8],
    contents: &[u8],
) -> Result<Vec<u8>, BackupError> {
    if salt.len() < MIN_SALT_LEN || salt.len() > u8::MAX as usize {
        return Err(BackupError::InvalidFormat);
    }
    let payload = encrypt(key, contents, AAD_V2)?;
    let mut result = HEADER_V2.to_vec();
    result.push(FORMAT_VERSION_V2);
    result.push(salt.len() as u8);
    result.extend_from_slice(salt);
    result.extend_from_slice(&payload.nonce);
    result.extend_from_slice(&payload.ciphertext);
    Ok(result)
}

/// Derives the key from the embedded salt and the master password, then decrypts.
/// A wrong password and a damaged file are indistinguishable by design (`Crypto` error).
pub fn restore_backup_v2(password: &str, backup: &[u8]) -> Result<Vec<u8>, BackupError> {
    if !backup.starts_with(HEADER_V2) || backup.len() < HEADER_V2.len() + 2 {
        return Err(BackupError::InvalidFormat);
    }
    let version = backup[HEADER_V2.len()];
    let salt_len = backup[HEADER_V2.len() + 1] as usize;
    let salt_start = HEADER_V2.len() + 2;
    let nonce_start = salt_start + salt_len;
    let cipher_start = nonce_start + NONCE_LEN;
    if version != FORMAT_VERSION_V2
        || salt_len < MIN_SALT_LEN
        || backup.len() < cipher_start + TAG_LEN
    {
        return Err(BackupError::InvalidFormat);
    }
    let key = derive_key(password, &backup[salt_start..nonce_start])?;
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&backup[nonce_start..cipher_start]);
    decrypt(
        &key,
        &EncryptedPayload {
            nonce,
            ciphertext: backup[cipher_start..].to_vec(),
        },
        AAD_V2,
    )
    .map_err(Into::into)
}
