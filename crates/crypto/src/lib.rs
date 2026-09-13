use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::{rngs::OsRng, RngCore};
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("key derivation failed")]
    KeyDerivation,
    #[error("encryption failed")]
    Encryption,
    #[error("decryption failed")]
    Decryption,
}

pub struct VaultKey(Zeroizing<[u8; 32]>);

impl AsRef<[u8]> for VaultKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

pub struct EncryptedPayload {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &str, salt: &[u8]) -> Result<VaultKey, CryptoError> {
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| CryptoError::KeyDerivation)?;
    Ok(VaultKey(key))
}

pub fn encrypt(
    key: &VaultKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptedPayload, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| CryptoError::Encryption)?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encryption)?;
    Ok(EncryptedPayload {
        nonce,
        ciphertext: encrypted,
    })
}

pub fn decrypt(
    key: &VaultKey,
    payload: &EncryptedPayload,
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| CryptoError::Decryption)?;
    cipher
        .decrypt(
            Nonce::from_slice(&payload.nonce),
            aes_gcm::aead::Payload {
                msg: &payload.ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decryption)
}
