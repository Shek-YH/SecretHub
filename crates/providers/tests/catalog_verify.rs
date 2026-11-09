use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use secrethub_providers::{
    bundled_serial, check_serial, embedded_public_key, evaluate_remote, verify_signature,
    SerialCheck, Verdict, MAX_CATALOG_BYTES,
};

// Test-only key pairs derived from fixed seeds; never used outside tests.
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_b64(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

fn sign(key: &SigningKey, data: &[u8]) -> String {
    STANDARD.encode(key.sign(data).to_bytes())
}

fn catalog(serial: u64) -> Vec<u8> {
    format!(
        r#"{{"schema":1,"serial":{serial},"publishedAt":"2026-10-17T00:00:00Z","providers":{{"deepseek":{{"models":[{{"id":"ds-new"}}]}}}}}}"#
    )
    .into_bytes()
}

fn newer() -> u64 {
    bundled_serial().serial + 1
}

#[test]
fn happy_path_applies_a_newer_signed_catalog() {
    let signer = key(1);
    let bytes = catalog(newer());
    let verdict =
        evaluate_remote(&bytes, &sign(&signer, &bytes), &public_b64(&signer), None).unwrap();
    match verdict {
        Verdict::Apply(overlay) => assert_eq!(overlay.serial, newer()),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn bad_signature_and_tampering_are_rejected() {
    let signer = key(1);
    let bytes = catalog(newer());
    let signature = sign(&signer, &bytes);
    let public = public_b64(&signer);
    let mut tampered = bytes.clone();
    tampered.extend_from_slice(b" ");
    assert!(evaluate_remote(&tampered, &signature, &public, None).is_err());
    let flipped = sign(&signer, &catalog(newer() + 1));
    assert!(evaluate_remote(&bytes, &flipped, &public, None).is_err());
    assert!(evaluate_remote(&bytes, "", &public, None).is_err());
    assert!(evaluate_remote(&bytes, "AAAA", &public, None).is_err());
    assert!(evaluate_remote(&bytes, "!!!not base64!!!", &public, None).is_err());
}

#[test]
fn wrong_key_is_rejected() {
    let signer = key(1);
    let other = key(2);
    let bytes = catalog(newer());
    assert!(evaluate_remote(&bytes, &sign(&signer, &bytes), &public_b64(&other), None).is_err());
    assert!(verify_signature("not-a-key", &bytes, &sign(&signer, &bytes)).is_err());
}

#[test]
fn serial_must_exceed_both_applied_and_bundled() {
    let signer = key(1);
    let public = public_b64(&signer);
    let evaluate = |serial: u64, applied: Option<u64>| {
        let bytes = catalog(serial);
        evaluate_remote(&bytes, &sign(&signer, &bytes), &public, applied)
    };
    // Older than the bundled serial: rollback even with nothing applied.
    assert!(evaluate(bundled_serial().serial - 1, None).is_err());
    // Equal to the bundled serial: nothing new.
    assert_eq!(
        evaluate(bundled_serial().serial, None).unwrap(),
        Verdict::UpToDate
    );
    // Older than what is already applied.
    assert!(evaluate(newer(), Some(newer() + 5)).is_err());
    assert_eq!(evaluate(newer(), Some(newer())).unwrap(), Verdict::UpToDate);
    assert!(matches!(
        evaluate(newer() + 6, Some(newer() + 5)).unwrap(),
        Verdict::Apply(_)
    ));
    assert_eq!(check_serial(newer(), None), SerialCheck::Newer);
    assert_eq!(
        check_serial(bundled_serial().serial, None),
        SerialCheck::Same
    );
    assert_eq!(check_serial(1, Some(2)), SerialCheck::Older);
}

#[test]
fn oversize_and_whitelist_violations_are_rejected_even_when_signed() {
    let signer = key(1);
    let public = public_b64(&signer);
    let big = vec![b' '; MAX_CATALOG_BYTES + 1];
    assert!(evaluate_remote(&big, &sign(&signer, &big), &public, None).is_err());
    let with_url = format!(
        r#"{{"schema":1,"serial":{},"publishedAt":"2026-10-17T00:00:00Z","providers":{{"deepseek":{{"models":[{{"id":"x","url":"https://evil.example"}}]}}}}}}"#,
        newer()
    )
    .into_bytes();
    assert!(evaluate_remote(&with_url, &sign(&signer, &with_url), &public, None).is_err());
}

#[test]
fn committed_public_key_is_a_valid_key() {
    let embedded = embedded_public_key();
    assert!(!embedded.is_empty() && !embedded.starts_with('#'));
    let bytes = STANDARD.decode(&embedded).unwrap();
    assert_eq!(bytes.len(), 32);
    assert!(ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into().unwrap()).is_ok());
    // A dev/placeholder key must never verify a signature made by an unrelated key.
    let bytes = catalog(newer());
    assert!(evaluate_remote(&bytes, &sign(&key(1), &bytes), &embedded, None).is_err());
}

/// End-to-end check against files produced by the maintainer scripts (catalog:keygen/sign).
/// Runs only when SECRETHUB_E2E_DIR points at a directory with the three files.
#[test]
fn externally_signed_catalog_is_accepted_and_tampering_rejected() {
    let Ok(dir) = std::env::var("SECRETHUB_E2E_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let bytes = std::fs::read(dir.join("model-catalog.json")).unwrap();
    let signature = std::fs::read_to_string(dir.join("model-catalog.json.sig")).unwrap();
    let public = std::fs::read_to_string(dir.join("catalog-public-key.txt")).unwrap();
    let public = public
        .lines()
        .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .unwrap()
        .to_owned();
    assert!(matches!(
        evaluate_remote(&bytes, &signature, &public, None).unwrap(),
        Verdict::Apply(_)
    ));
    let mut tampered = bytes.clone();
    tampered.extend_from_slice(b"\n");
    assert!(evaluate_remote(&tampered, &signature, &public, None).is_err());
}
