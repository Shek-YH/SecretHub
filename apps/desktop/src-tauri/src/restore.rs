//! Restore the vault from an encrypted backup.
//!
//! Everything that can fail without side effects (reading, decrypting, validating the
//! database, checking the password) happens before the live vault is touched. The current
//! vault files are only ever moved into a `vault-backup-<stamp>` folder, never deleted.

use secrethub_backup::{detect_format, restore_backup_v2, BackupError, BackupFormat};
use secrethub_core::{VaultError, VaultService};
use secrethub_storage::Database;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

pub const MAX_BACKUP_BYTES: u64 = 1024 * 1024 * 1024;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

/// Failures carry stable codes that the UI turns into localized messages.
#[derive(Debug, PartialEq, Eq)]
pub enum RestoreError {
    TooLarge,
    Unreadable,
    InvalidBackup,
    BadPasswordOrCorrupt,
    NotSqlite,
    NotVault,
    LegacyOtherVault,
    FilesBusy,
    Io(String),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("restore_too_large"),
            Self::Unreadable => formatter.write_str("restore_unreadable"),
            Self::InvalidBackup => formatter.write_str("restore_invalid_backup"),
            Self::BadPasswordOrCorrupt => formatter.write_str("restore_bad_password"),
            Self::NotSqlite => formatter.write_str("restore_not_sqlite"),
            Self::NotVault => formatter.write_str("restore_not_vault"),
            Self::LegacyOtherVault => formatter.write_str("restore_legacy_other_vault"),
            Self::FilesBusy => formatter.write_str("restore_files_busy"),
            Self::Io(detail) => write!(formatter, "restore_failed: {detail}"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Retry {
    pub attempts: u32,
    pub delay: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        // Windows keeps files locked while another process (for example the MCP server) reads them.
        Self {
            attempts: 6,
            delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug)]
pub struct Restored {
    /// Folder holding the previous vault files; `None` when there was no previous vault file.
    pub backup_dir: Option<PathBuf>,
}

pub fn read_backup_file(path: &Path) -> Result<Vec<u8>, RestoreError> {
    let metadata = fs::metadata(path).map_err(|_| RestoreError::Unreadable)?;
    if !metadata.is_file() {
        return Err(RestoreError::Unreadable);
    }
    if metadata.len() > MAX_BACKUP_BYTES {
        return Err(RestoreError::TooLarge);
    }
    fs::read(path).map_err(|_| RestoreError::Unreadable)
}

/// SHB2 uses the salt in the file and the given password; SHB1 is only accepted from the
/// same, currently unlocked vault.
pub fn decrypt_backup(
    backup: &[u8],
    password: &str,
    current: Option<&VaultService>,
) -> Result<Vec<u8>, RestoreError> {
    match detect_format(backup) {
        Some(BackupFormat::V2) => {
            restore_backup_v2(password, backup).map_err(|error| match error {
                BackupError::InvalidFormat => RestoreError::InvalidBackup,
                BackupError::Crypto(_) => RestoreError::BadPasswordOrCorrupt,
            })
        }
        Some(BackupFormat::V1) => match current {
            Some(vault) if vault.is_unlocked() => vault
                .decrypt_legacy_backup(backup)
                .map_err(|_| RestoreError::LegacyOtherVault),
            _ => Err(RestoreError::LegacyOtherVault),
        },
        None => Err(RestoreError::InvalidBackup),
    }
}

/// Opens the decrypted database as a temporary file and checks that it is a SecretHub vault
/// whose password verifier matches `password`.
pub fn verify_candidate(
    data_dir: &Path,
    plaintext: &[u8],
    password: &str,
) -> Result<(), RestoreError> {
    if plaintext.len() < SQLITE_HEADER.len() || &plaintext[..SQLITE_HEADER.len()] != SQLITE_HEADER {
        return Err(RestoreError::NotSqlite);
    }
    let check_dir = data_dir.join(format!("restore-check-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&check_dir).map_err(|error| RestoreError::Io(error.to_string()))?;
    let result = (|| {
        let candidate = check_dir.join("vault.sqlite");
        fs::write(&candidate, plaintext).map_err(|error| RestoreError::Io(error.to_string()))?;
        let database = Database::open(&candidate).map_err(|_| RestoreError::NotSqlite)?;
        let mut vault = VaultService::open(database).map_err(|_| RestoreError::NotVault)?;
        vault.unlock(password).map_err(|error| match error {
            VaultError::InvalidPassword => RestoreError::BadPasswordOrCorrupt,
            _ => RestoreError::NotVault,
        })
    })();
    // The temporary copy is our own scratch file; the connection is dropped before cleanup.
    let _ = fs::remove_dir_all(&check_dir);
    result
}

fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn vault_files(db_path: &Path) -> [PathBuf; 3] {
    [
        db_path.to_path_buf(),
        sidecar(db_path, "-wal"),
        sidecar(db_path, "-shm"),
    ]
}

fn rename_retry(from: &Path, to: &Path, retry: Retry) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..retry.attempts.max(1) {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) => last = Some(error),
        }
        if attempt + 1 < retry.attempts {
            thread::sleep(retry.delay);
        }
    }
    Err(last.expect("at least one attempt"))
}

fn classify_io(error: std::io::Error) -> RestoreError {
    // 5 = access denied, 32 = sharing violation, 33 = lock violation (Windows).
    if matches!(error.kind(), std::io::ErrorKind::PermissionDenied)
        || matches!(error.raw_os_error(), Some(5 | 32 | 33))
    {
        RestoreError::FilesBusy
    } else {
        RestoreError::Io(error.to_string())
    }
}

/// Puts `plaintext` in place of the live vault files. The previous files go to
/// `<data dir>/vault-backup-<stamp>/`. On failure the previous files are moved back.
pub fn swap_files(
    db_path: &Path,
    plaintext: &[u8],
    stamp: &str,
    retry: Retry,
) -> Result<Restored, RestoreError> {
    let data_dir = db_path.parent().ok_or(RestoreError::Unreadable)?;
    let restoring = sidecar(db_path, ".restoring");
    let write_result = (|| {
        let mut file = fs::File::create(&restoring)?;
        file.write_all(plaintext)?;
        file.sync_all()
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&restoring);
        return Err(RestoreError::Io(error.to_string()));
    }

    let existing: Vec<PathBuf> = vault_files(db_path)
        .into_iter()
        .filter(|path| path.exists())
        .collect();
    let mut backup_dir = None;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let undo = |moved: &[(PathBuf, PathBuf)], backup_dir: &Option<PathBuf>| {
        for (original, saved) in moved.iter().rev() {
            let _ = fs::rename(saved, original);
        }
        if let Some(dir) = backup_dir {
            // Only removes the folder when it is empty again.
            let _ = fs::remove_dir(dir);
        }
        let _ = fs::remove_file(&restoring);
    };

    if !existing.is_empty() {
        let mut dir = data_dir.join(format!("vault-backup-{stamp}"));
        let mut counter = 2;
        while dir.exists() {
            dir = data_dir.join(format!("vault-backup-{stamp}-{counter}"));
            counter += 1;
        }
        if let Err(error) = fs::create_dir_all(&dir) {
            let _ = fs::remove_file(&restoring);
            return Err(RestoreError::Io(error.to_string()));
        }
        backup_dir = Some(dir.clone());
        for original in &existing {
            let saved = dir.join(original.file_name().expect("vault file name"));
            if let Err(error) = rename_retry(original, &saved, retry) {
                undo(&moved, &backup_dir);
                return Err(classify_io(error));
            }
            moved.push((original.clone(), saved));
        }
    }
    if let Err(error) = rename_retry(&restoring, db_path, retry) {
        undo(&moved, &backup_dir);
        return Err(classify_io(error));
    }
    Ok(Restored { backup_dir })
}

/// Best effort: sets the restored files aside inside the backup folder and puts the previous
/// vault files back where they were.
fn rollback(db_path: &Path, restored: &Restored) {
    let Some(dir) = &restored.backup_dir else {
        return;
    };
    for current in vault_files(db_path) {
        if current.exists() {
            if let Some(name) = current.file_name() {
                let mut aside = OsString::from("restored-failed-");
                aside.push(name);
                let _ = fs::rename(&current, dir.join(aside));
            }
        }
    }
    for original in vault_files(db_path) {
        if let Some(name) = original.file_name() {
            let saved = dir.join(name);
            if saved.exists() {
                let _ = fs::rename(&saved, &original);
            }
        }
    }
}

fn open_service(db_path: &Path) -> Option<VaultService> {
    Database::open(db_path)
        .ok()
        .and_then(|database| VaultService::open(database).ok())
}

/// Restores `backup` into `slot`. On success `slot` holds the restored vault, unlocked with
/// `password`. Errors before the swap leave `slot` and every file untouched; later errors put
/// the previous files back and leave a locked vault in `slot`.
pub fn restore_vault(
    slot: &mut Option<VaultService>,
    db_path: &Path,
    backup: &[u8],
    password: &str,
    stamp: &str,
    retry: Retry,
) -> Result<Restored, RestoreError> {
    let data_dir = db_path.parent().ok_or(RestoreError::Unreadable)?;
    let plaintext = decrypt_backup(backup, password, slot.as_ref())?;
    verify_candidate(data_dir, &plaintext, password)?;

    // Releases the live SQLite connection so the files can be moved.
    drop(slot.take());
    let restored = match swap_files(db_path, &plaintext, stamp, retry) {
        Ok(restored) => restored,
        Err(error) => {
            *slot = open_service(db_path);
            return Err(error);
        }
    };
    let reopened = open_service(db_path)
        .ok_or(RestoreError::NotVault)
        .and_then(|mut vault| {
            vault
                .unlock(password)
                .map_err(|_| RestoreError::BadPasswordOrCorrupt)?;
            Ok(vault)
        });
    match reopened {
        Ok(vault) => {
            *slot = Some(vault);
            Ok(restored)
        }
        Err(error) => {
            rollback(db_path, &restored);
            *slot = open_service(db_path);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_backup::{create_backup, create_backup_v2};
    use secrethub_core::NewSecretInput;
    use secrethub_crypto::derive_key;
    use std::fs;

    const PASSWORD: &str = "correct horse battery staple";
    const STAMP: &str = "20261109-101500";
    const FAST: Retry = Retry {
        attempts: 2,
        delay: Duration::from_millis(10),
    };

    fn new_vault(dir: &Path, password: &str, secret: Option<(&str, &str)>) -> VaultService {
        let database = Database::open(dir.join("vault.sqlite")).unwrap();
        let mut vault = VaultService::open(database).unwrap();
        vault.setup_master(password).unwrap();
        if let Some((name, value)) = secret {
            vault
                .create_secret(NewSecretInput {
                    name: name.into(),
                    provider_id: "generic".into(),
                    env_key: "FAKE_KEY".into(),
                    description: String::new(),
                    tags: Vec::new(),
                    value: value.into(),
                })
                .unwrap();
        }
        vault
    }

    fn names(vault: &VaultService) -> Vec<String> {
        vault
            .list_metadata()
            .unwrap()
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    fn read_value(vault: &VaultService) -> String {
        let id = vault.list_metadata().unwrap()[0].id.clone();
        vault.read_secret(&id).unwrap()
    }

    fn backup_dirs(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("vault-backup-")
            })
            .collect()
    }

    #[test]
    fn shb2_round_trips_into_a_fresh_directory_with_only_the_password() {
        let source_dir = tempfile::tempdir().unwrap();
        let source = new_vault(
            source_dir.path(),
            PASSWORD,
            Some(("Restored key", "fake-value-1")),
        );
        let backup = source.encrypted_backup().unwrap();
        assert_eq!(&backup[..4], b"SHB2");
        drop(source);

        let fresh = tempfile::tempdir().unwrap();
        let db_path = fresh.path().join("vault.sqlite");
        let mut slot = None;
        let restored = restore_vault(&mut slot, &db_path, &backup, PASSWORD, STAMP, FAST).unwrap();
        assert!(restored.backup_dir.is_none());
        let vault = slot.as_ref().unwrap();
        assert!(vault.is_unlocked());
        assert_eq!(names(vault), ["Restored key"]);
        assert_eq!(read_value(vault), "fake-value-1");
        assert!(!sidecar(&db_path, ".restoring").exists());
        assert!(fs::read_dir(fresh.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("restore-check-")));
    }

    #[test]
    fn replaced_vault_is_moved_to_a_backup_folder_and_never_deleted() {
        let source_dir = tempfile::tempdir().unwrap();
        let source = new_vault(
            source_dir.path(),
            PASSWORD,
            Some(("From backup", "fake-from-backup")),
        );
        let backup = source.encrypted_backup().unwrap();
        drop(source);

        let live_dir = tempfile::tempdir().unwrap();
        let db_path = live_dir.path().join("vault.sqlite");
        let live = new_vault(
            live_dir.path(),
            "another long password",
            Some(("Old live", "fake-old-live")),
        );
        let mut slot = Some(live);
        let restored = restore_vault(&mut slot, &db_path, &backup, PASSWORD, STAMP, FAST).unwrap();

        assert_eq!(names(slot.as_ref().unwrap()), ["From backup"]);
        let old_dir = restored.backup_dir.unwrap();
        assert_eq!(backup_dirs(live_dir.path()), vec![old_dir.clone()]);
        assert!(old_dir.ends_with(format!("vault-backup-{STAMP}")));
        let mut old =
            VaultService::open(Database::open(old_dir.join("vault.sqlite")).unwrap()).unwrap();
        old.unlock("another long password").unwrap();
        assert_eq!(names(&old), ["Old live"]);
        assert_eq!(read_value(&old), "fake-old-live");
    }

    #[test]
    fn wrong_password_and_tampered_files_leave_the_live_vault_untouched() {
        let source_dir = tempfile::tempdir().unwrap();
        let source = new_vault(source_dir.path(), PASSWORD, Some(("From backup", "fake")));
        let backup = source.encrypted_backup().unwrap();
        drop(source);

        let live_dir = tempfile::tempdir().unwrap();
        let db_path = live_dir.path().join("vault.sqlite");
        let live = new_vault(
            live_dir.path(),
            "another long password",
            Some(("Old live", "fake-old-live")),
        );
        live.encrypted_backup().unwrap(); // checkpoints the live database before the comparison
        let before = fs::read(&db_path).unwrap();
        let mut slot = Some(live);

        let wrong = restore_vault(
            &mut slot,
            &db_path,
            &backup,
            "wrong password value",
            STAMP,
            FAST,
        );
        assert_eq!(wrong.unwrap_err(), RestoreError::BadPasswordOrCorrupt);
        let mut tampered = backup.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert_eq!(
            restore_vault(&mut slot, &db_path, &tampered, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::BadPasswordOrCorrupt
        );
        assert_eq!(
            restore_vault(
                &mut slot,
                &db_path,
                b"not a backup at all",
                PASSWORD,
                STAMP,
                FAST
            )
            .unwrap_err(),
            RestoreError::InvalidBackup
        );

        assert!(slot.as_ref().unwrap().is_unlocked());
        assert_eq!(names(slot.as_ref().unwrap()), ["Old live"]);
        assert!(backup_dirs(live_dir.path()).is_empty());
        assert_eq!(fs::read(&db_path).unwrap(), before);
    }

    #[test]
    fn decrypted_data_must_be_a_sqlite_vault_with_a_matching_verifier() {
        let salt = secrethub_crypto::generate_salt();
        let key = derive_key(PASSWORD, &salt).unwrap();
        let live_dir = tempfile::tempdir().unwrap();
        let db_path = live_dir.path().join("vault.sqlite");
        let live = new_vault(live_dir.path(), PASSWORD, Some(("Live", "fake")));
        let mut slot = Some(live);

        let not_sqlite =
            create_backup_v2(&key, &salt, b"plain text, definitely not sqlite").unwrap();
        assert_eq!(
            restore_vault(&mut slot, &db_path, &not_sqlite, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::NotSqlite
        );

        // A valid SQLite file that was never set up as a vault has no verifier.
        let blank_dir = tempfile::tempdir().unwrap();
        drop(Database::open(blank_dir.path().join("vault.sqlite")).unwrap());
        let blank = fs::read(blank_dir.path().join("vault.sqlite")).unwrap();
        let blank_backup = create_backup_v2(&key, &salt, &blank).unwrap();
        assert_eq!(
            restore_vault(&mut slot, &db_path, &blank_backup, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::NotVault
        );

        // Truncated SQLite header bytes with a correct key but garbage afterwards.
        let mut garbage = SQLITE_HEADER.to_vec();
        garbage.extend_from_slice(&[7u8; 4096]);
        let garbage_backup = create_backup_v2(&key, &salt, &garbage).unwrap();
        assert!(
            restore_vault(&mut slot, &db_path, &garbage_backup, PASSWORD, STAMP, FAST).is_err()
        );

        assert_eq!(names(slot.as_ref().unwrap()), ["Live"]);
        assert!(backup_dirs(live_dir.path()).is_empty());
    }

    #[test]
    fn legacy_shb1_restores_only_into_the_same_unlocked_vault() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("vault.sqlite");
        let mut vault = new_vault(dir.path(), PASSWORD, Some(("Before", "fake-before")));
        vault.encrypted_backup().unwrap(); // checkpoint so the file holds the first secret
        let (salt, _) = Database::open(&db_path)
            .unwrap()
            .get_vault_meta()
            .unwrap()
            .unwrap();
        let key = derive_key(PASSWORD, &salt).unwrap();
        let v1 = create_backup(&key, &fs::read(&db_path).unwrap()).unwrap();
        vault
            .create_secret(NewSecretInput {
                name: "After".into(),
                provider_id: "generic".into(),
                env_key: "FAKE_KEY_2".into(),
                description: String::new(),
                tags: Vec::new(),
                value: "fake-after".into(),
            })
            .unwrap();
        assert_eq!(names(&vault).len(), 2);

        // A locked vault (or none at all) cannot open an SHB1 file.
        let mut locked = None;
        assert_eq!(
            restore_vault(&mut locked, &db_path, &v1, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::LegacyOtherVault
        );
        vault.lock();
        let mut slot = Some(vault);
        assert_eq!(
            restore_vault(&mut slot, &db_path, &v1, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::LegacyOtherVault
        );
        slot.as_mut().unwrap().unlock(PASSWORD).unwrap();

        // Unlocked, same vault: rolled back to one secret.
        restore_vault(&mut slot, &db_path, &v1, PASSWORD, STAMP, FAST).unwrap();
        assert_eq!(names(slot.as_ref().unwrap()), ["Before"]);
        assert_eq!(backup_dirs(dir.path()).len(), 1);

        // Another vault's SHB1 file cannot be decrypted with this vault's key.
        let other_dir = tempfile::tempdir().unwrap();
        let other = new_vault(
            other_dir.path(),
            "a different long password",
            Some(("Other", "fake")),
        );
        other.encrypted_backup().unwrap();
        let (other_salt, _) = Database::open(other_dir.path().join("vault.sqlite"))
            .unwrap()
            .get_vault_meta()
            .unwrap()
            .unwrap();
        let other_key = derive_key("a different long password", &other_salt).unwrap();
        let other_v1 = create_backup(
            &other_key,
            &fs::read(other_dir.path().join("vault.sqlite")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            restore_vault(&mut slot, &db_path, &other_v1, PASSWORD, STAMP, FAST).unwrap_err(),
            RestoreError::LegacyOtherVault
        );
        assert_eq!(names(slot.as_ref().unwrap()), ["Before"]);
    }

    #[test]
    fn oversized_or_missing_backup_files_are_rejected_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_backup_file(&dir.path().join("missing.secrethub-backup")).unwrap_err(),
            RestoreError::Unreadable
        );
        assert_eq!(
            read_backup_file(dir.path()).unwrap_err(),
            RestoreError::Unreadable
        );
        let small = dir.path().join("a.secrethub-backup");
        fs::write(&small, b"SHB2").unwrap();
        assert_eq!(read_backup_file(&small).unwrap(), b"SHB2");
    }

    #[cfg(windows)]
    #[test]
    fn locked_vault_files_report_a_busy_error_and_keep_the_previous_vault() {
        let source_dir = tempfile::tempdir().unwrap();
        let source = new_vault(source_dir.path(), PASSWORD, Some(("From backup", "fake")));
        let backup = source.encrypted_backup().unwrap();
        drop(source);

        let live_dir = tempfile::tempdir().unwrap();
        let db_path = live_dir.path().join("vault.sqlite");
        let live = new_vault(
            live_dir.path(),
            PASSWORD,
            Some(("Old live", "fake-old-live")),
        );
        let mut slot = Some(live);
        // Another process (for example the MCP server) keeps the database open.
        let _holder = Database::open(&db_path).unwrap();
        let error = restore_vault(&mut slot, &db_path, &backup, PASSWORD, STAMP, FAST).unwrap_err();
        assert_eq!(error, RestoreError::FilesBusy);
        assert!(db_path.exists());
        assert!(!sidecar(&db_path, ".restoring").exists());
        assert!(backup_dirs(live_dir.path()).is_empty());
        // The vault is reopened (locked) with the previous data.
        let mut reopened = slot.take().unwrap();
        reopened.unlock(PASSWORD).unwrap();
        assert_eq!(names(&reopened), ["Old live"]);
    }
}
