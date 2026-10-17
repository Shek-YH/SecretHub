//! Signature verification and anti-rollback policy for the remote model catalog.
//! Pure functions: bytes in, verdict out. Fetching and caching live in the desktop crate.

use crate::catalog::{bundled_serial, parse_overlay, CatalogError, Overlay, MAX_CATALOG_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Public key compiled into the app (`catalog-public-key.txt`): base64 of the raw
/// 32-byte Ed25519 key; lines starting with `#` are comments.
const PUBLIC_KEY_FILE: &str =
    include_str!("../../../packages/provider-definitions/catalog-public-key.txt");

pub fn embedded_public_key() -> String {
    PUBLIC_KEY_FILE
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or_default()
        .to_owned()
}

fn fail<T>(message: &str) -> Result<T, CatalogError> {
    Err(CatalogError(message.to_owned()))
}

/// Verifies a detached Ed25519 signature (base64) over the exact file bytes.
pub fn verify_signature(
    public_key_b64: &str,
    data: &[u8],
    signature_b64: &str,
) -> Result<(), CatalogError> {
    let key_bytes: [u8; 32] = STANDARD
        .decode(public_key_b64.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .map_or_else(|| fail("public key is not 32 bytes of base64"), Ok)?;
    let key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| CatalogError("public key is invalid".to_owned()))?;
    let signature_bytes: [u8; 64] = STANDARD
        .decode(signature_b64.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .map_or_else(|| fail("signature is not 64 bytes of base64"), Ok)?;
    key.verify(data, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| CatalogError("signature does not match".to_owned()))
}

/// Outcome of weighing a verified serial against what is already in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialCheck {
    Newer,
    Same,
    Older,
}

/// The candidate must exceed both the serial in use and the serial bundled with this build.
pub fn check_serial(candidate: u64, applied: Option<u64>) -> SerialCheck {
    let floor = applied.unwrap_or(0).max(bundled_serial().serial);
    match candidate.cmp(&floor) {
        std::cmp::Ordering::Greater => SerialCheck::Newer,
        std::cmp::Ordering::Equal => SerialCheck::Same,
        std::cmp::Ordering::Less => SerialCheck::Older,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Apply(Overlay),
    /// Valid and signed, but not newer than what is in use.
    UpToDate,
}

/// Size cap, then signature, then strict parse, then anti-rollback. Unauthenticated
/// bytes are never parsed.
pub fn evaluate_remote(
    bytes: &[u8],
    signature_b64: &str,
    public_key_b64: &str,
    applied_serial: Option<u64>,
) -> Result<Verdict, CatalogError> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return fail("file exceeds 1 MB");
    }
    verify_signature(public_key_b64, bytes, signature_b64)?;
    let overlay = parse_overlay(bytes)?;
    match check_serial(overlay.serial, applied_serial) {
        SerialCheck::Newer => Ok(Verdict::Apply(overlay)),
        SerialCheck::Same => Ok(Verdict::UpToDate),
        SerialCheck::Older => fail("serial is older than the version in use (rollback)"),
    }
}
