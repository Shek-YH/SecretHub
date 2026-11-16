use secrethub_backup::{
    create_backup, create_backup_v2, detect_format, restore_backup, restore_backup_v2, BackupError,
    BackupFormat,
};
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

#[test]
fn v2_backup_carries_its_salt_and_restores_with_only_the_password() {
    let salt = generate_salt();
    let key = derive_key("correct horse battery staple", &salt).unwrap();
    let backup = create_backup_v2(&key, &salt, b"encrypted-vault-fixture").unwrap();
    assert_eq!(detect_format(&backup), Some(BackupFormat::V2));
    assert_eq!(&backup[..4], b"SHB2");
    assert_eq!(backup[5] as usize, salt.len());
    assert_eq!(&backup[6..6 + salt.len()], salt.as_slice());
    assert!(!backup
        .windows(b"encrypted-vault-fixture".len())
        .any(|window| window == b"encrypted-vault-fixture"));
    // No key material from the original vault is needed, only the password.
    assert_eq!(
        restore_backup_v2("correct horse battery staple", &backup).unwrap(),
        b"encrypted-vault-fixture"
    );
}

#[test]
fn v2_backup_rejects_wrong_passwords_tampering_and_malformed_headers() {
    let salt = generate_salt();
    let key = derive_key("correct horse battery staple", &salt).unwrap();
    let backup = create_backup_v2(&key, &salt, b"fixture").unwrap();
    assert!(matches!(
        restore_backup_v2("wrong password value", &backup),
        Err(BackupError::Crypto(_))
    ));
    let mut tampered = backup.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(matches!(
        restore_backup_v2("correct horse battery staple", &tampered),
        Err(BackupError::Crypto(_))
    ));
    let mut bad_salt = backup.clone();
    bad_salt[6] ^= 0x01;
    assert!(matches!(
        restore_backup_v2("correct horse battery staple", &bad_salt),
        Err(BackupError::Crypto(_))
    ));
    let mut bad_version = backup.clone();
    bad_version[4] = 9;
    assert!(matches!(
        restore_backup_v2("correct horse battery staple", &bad_version),
        Err(BackupError::InvalidFormat)
    ));
    assert!(matches!(
        restore_backup_v2("x", &backup[..10]),
        Err(BackupError::InvalidFormat)
    ));
    assert_eq!(detect_format(b"nonsense"), None);
    let v1 = create_backup(&key, b"fixture").unwrap();
    assert_eq!(detect_format(&v1), Some(BackupFormat::V1));
    assert!(matches!(
        restore_backup_v2("correct horse battery staple", &v1),
        Err(BackupError::InvalidFormat)
    ));
}
