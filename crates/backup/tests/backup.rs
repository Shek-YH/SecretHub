use secrethub_backup::{create_backup, restore_backup};
use secrethub_crypto::{derive_key, generate_salt};

#[test]
fn backup_bytes_are_encrypted_and_round_trip() {
    let salt = generate_salt();
    let key = derive_key("correct horse battery staple", &salt).unwrap();
    let backup = create_backup(&key, b"encrypted-vault-fixture").unwrap();

    assert!(!backup
        .windows(b"encrypted-vault-fixture".len())
        .any(|window| window == b"encrypted-vault-fixture"));
    assert_eq!(
        restore_backup(&key, &backup).unwrap(),
        b"encrypted-vault-fixture"
    );
}
