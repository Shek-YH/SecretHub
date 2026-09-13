use secrethub_crypto::{decrypt, derive_key, encrypt, generate_salt};

#[test]
fn encrypts_and_decrypts_without_exposing_plaintext_in_ciphertext() {
    let salt = generate_salt();
    let key = derive_key("correct horse battery staple", &salt).unwrap();
    let encrypted = encrypt(&key, b"fixture-secret-value", b"secret:v1").unwrap();

    assert_ne!(encrypted.ciphertext, b"fixture-secret-value");
    assert_eq!(decrypt(&key, &encrypted, b"secret:v1").unwrap(), b"fixture-secret-value");
}

#[test]
fn rejects_wrong_password_and_tampering() {
    let salt = generate_salt();
    let key = derive_key("correct horse battery staple", &salt).unwrap();
    let wrong = derive_key("wrong password", &salt).unwrap();
    let mut encrypted = encrypt(&key, b"fixture-secret-value", b"secret:v1").unwrap();

    assert!(decrypt(&wrong, &encrypted, b"secret:v1").is_err());
    encrypted.ciphertext[0] ^= 1;
    assert!(decrypt(&key, &encrypted, b"secret:v1").is_err());
}
