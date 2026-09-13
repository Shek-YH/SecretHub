//! SQLite repository for metadata and encrypted secret payloads.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretMetadata {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewSecret {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

pub struct UpdatedSecret {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: String,
    pub operation: String,
    pub secret_id: Option<String>,
    pub value_hash: Option<String>,
    pub result: String,
    pub created_at: i64,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub secret_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub path: String,
    pub display_name: String,
    pub last_used_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub locale: String,
    pub theme: String,
    pub auto_lock_minutes: u32,
    pub clipboard_timeout_seconds: u32,
    pub require_auth_before_export: bool,
    pub require_auth_before_reveal: bool,
}

pub type VaultMeta = (Vec<u8>, Vec<u8>);
pub type EncryptedParts = (Vec<u8>, Vec<u8>);

pub struct Database {
    connection: Connection,
    database_path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let database_path = path.as_ref().to_path_buf();
        let connection = Connection::open(&database_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS vault_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                salt BLOB NOT NULL,
                verifier BLOB NOT NULL,
                schema_version INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE IF NOT EXISTS secrets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                env_key TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                tags_json TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'unknown',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS secret_payloads (
                secret_id TEXT PRIMARY KEY REFERENCES secrets(id) ON DELETE CASCADE,
                ciphertext BLOB NOT NULL,
                nonce BLOB NOT NULL,
                algorithm TEXT NOT NULL DEFAULT 'AES-256-GCM',
                version INTEGER NOT NULL DEFAULT 1
             );
             CREATE INDEX IF NOT EXISTS idx_secrets_search ON secrets(name, provider_id, env_key);
             CREATE TABLE IF NOT EXISTS audit_events (
                id TEXT PRIMARY KEY,
                operation TEXT NOT NULL,
                secret_id TEXT,
                value_hash TEXT,
                result TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}'
             );
             CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS profile_secrets (
                profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                secret_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY(profile_id, secret_id)
             );
             CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                last_used_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                locale TEXT NOT NULL DEFAULT 'zh-CN',
                theme TEXT NOT NULL DEFAULT 'dark',
                auto_lock_minutes INTEGER NOT NULL DEFAULT 15,
                clipboard_timeout_seconds INTEGER NOT NULL DEFAULT 30,
                require_auth_before_export INTEGER NOT NULL DEFAULT 1,
                require_auth_before_reveal INTEGER NOT NULL DEFAULT 1
             );",
        )?;
        Ok(Self {
            connection,
            database_path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.database_path
    }

    pub fn checkpoint(&self) -> Result<(), StorageError> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(FULL);")?;
        Ok(())
    }

    pub fn set_vault_meta(&self, salt: &[u8], verifier: &[u8]) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO vault_meta(id, salt, verifier) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET salt = excluded.salt, verifier = excluded.verifier",
            params![salt, verifier],
        )?;
        Ok(())
    }

    pub fn get_vault_meta(&self) -> Result<Option<VaultMeta>, StorageError> {
        self.connection
            .query_row(
                "SELECT salt, verifier FROM vault_meta WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn create_secret(&self, secret: NewSecret) -> Result<String, StorageError> {
        let id = format!("secret-{}", uuid::Uuid::new_v4());
        let now = unix_time();
        let tags_json = serde_json::to_string(&secret.tags)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO secrets(id, name, provider_id, env_key, description, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![id, secret.name, secret.provider_id, secret.env_key, secret.description, tags_json, now],
        )?;
        self.connection.execute(
            "INSERT INTO secret_payloads(secret_id, ciphertext, nonce) VALUES (?1, ?2, ?3)",
            params![id, secret.ciphertext, secret.nonce],
        )?;
        Ok(id)
    }

    pub fn get_metadata(&self, id: &str) -> Result<Option<SecretMetadata>, StorageError> {
        self.connection.query_row(
            "SELECT id, name, provider_id, env_key, description, tags_json, status, created_at, updated_at FROM secrets WHERE id = ?1",
            params![id],
            metadata_from_row,
        ).optional().map_err(Into::into)
    }

    pub fn list_metadata(&self) -> Result<Vec<SecretMetadata>, StorageError> {
        let mut statement = self.connection.prepare("SELECT id, name, provider_id, env_key, description, tags_json, status, created_at, updated_at FROM secrets ORDER BY updated_at DESC")?;
        let rows = statement
            .query_map([], metadata_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn search_metadata(&self, query: &str) -> Result<Vec<SecretMetadata>, StorageError> {
        let pattern = format!("%{}%", query);
        let mut statement = self.connection.prepare("SELECT id, name, provider_id, env_key, description, tags_json, status, created_at, updated_at FROM secrets WHERE name LIKE ?1 COLLATE NOCASE OR provider_id LIKE ?1 COLLATE NOCASE OR env_key LIKE ?1 COLLATE NOCASE OR description LIKE ?1 COLLATE NOCASE OR tags_json LIKE ?1 COLLATE NOCASE ORDER BY updated_at DESC")?;
        let rows = statement
            .query_map(params![pattern], metadata_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_payload(&self, id: &str) -> Result<Option<EncryptedParts>, StorageError> {
        self.connection
            .query_row(
                "SELECT ciphertext, nonce FROM secret_payloads WHERE secret_id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn delete_secret(&self, id: &str) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM secrets WHERE id = ?1", params![id])?
            > 0)
    }

    pub fn update_secret(&self, id: &str, secret: UpdatedSecret) -> Result<bool, StorageError> {
        let now = unix_time();
        let tags_json = serde_json::to_string(&secret.tags)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        let updated = self.connection.execute("UPDATE secrets SET name = ?1, provider_id = ?2, env_key = ?3, description = ?4, tags_json = ?5, updated_at = ?6 WHERE id = ?7", params![secret.name, secret.provider_id, secret.env_key, secret.description, tags_json, now, id])?;
        if updated == 0 {
            return Ok(false);
        }
        self.connection.execute(
            "UPDATE secret_payloads SET ciphertext = ?1, nonce = ?2 WHERE secret_id = ?3",
            params![secret.ciphertext, secret.nonce, id],
        )?;
        Ok(true)
    }

    pub fn record_audit(
        &self,
        operation: &str,
        secret_id: Option<&str>,
        value_hash: Option<&str>,
        result: &str,
        metadata_json: &str,
    ) -> Result<(), StorageError> {
        let id = format!("audit-{}", uuid::Uuid::new_v4());
        self.connection.execute("INSERT INTO audit_events(id, operation, secret_id, value_hash, result, created_at, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![id, operation, secret_id, value_hash, result, unix_time(), metadata_json])?;
        Ok(())
    }

    pub fn list_audit(&self) -> Result<Vec<AuditEvent>, StorageError> {
        let mut statement = self.connection.prepare("SELECT id, operation, secret_id, value_hash, result, created_at, metadata_json FROM audit_events ORDER BY created_at DESC")?;
        let rows = statement
            .query_map([], |row| {
                Ok(AuditEvent {
                    id: row.get(0)?,
                    operation: row.get(1)?,
                    secret_id: row.get(2)?,
                    value_hash: row.get(3)?,
                    result: row.get(4)?,
                    created_at: row.get(5)?,
                    metadata_json: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn save_profile(&self, profile: &StoredProfile) -> Result<(), StorageError> {
        self.connection.execute("INSERT INTO profiles(id, name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, updated_at = excluded.updated_at", params![profile.id, profile.name, profile.description, profile.created_at, profile.updated_at])?;
        self.connection.execute(
            "DELETE FROM profile_secrets WHERE profile_id = ?1",
            params![profile.id],
        )?;
        for (position, secret_id) in profile.secret_ids.iter().enumerate() {
            self.connection.execute(
                "INSERT INTO profile_secrets(profile_id, secret_id, position) VALUES (?1, ?2, ?3)",
                params![profile.id, secret_id, position as i64],
            )?;
        }
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<StoredProfile>, StorageError> {
        let mut profiles = Vec::new();
        let mut statement = self.connection.prepare("SELECT id, name, description, created_at, updated_at FROM profiles ORDER BY updated_at DESC")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, name, description, created_at, updated_at) in rows {
            let mut ids = self
                .connection
                .prepare(
                    "SELECT secret_id FROM profile_secrets WHERE profile_id = ?1 ORDER BY position",
                )?
                .query_map(params![id], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            profiles.push(StoredProfile {
                id,
                name,
                description,
                secret_ids: std::mem::take(&mut ids),
                created_at,
                updated_at,
            });
        }
        Ok(profiles)
    }

    pub fn record_project(
        &self,
        path: &str,
        display_name: &str,
    ) -> Result<ProjectRecord, StorageError> {
        let id = format!("project-{}", uuid::Uuid::new_v4());
        let now = unix_time();
        self.connection.execute("INSERT INTO projects(id, path, display_name, last_used_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(path) DO UPDATE SET display_name = excluded.display_name, last_used_at = excluded.last_used_at", params![id, path, display_name, now])?;
        self.connection
            .query_row(
                "SELECT id, path, display_name, last_used_at FROM projects WHERE path = ?1",
                params![path],
                |row| {
                    Ok(ProjectRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        display_name: row.get(2)?,
                        last_used_at: row.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, display_name, last_used_at FROM projects ORDER BY last_used_at DESC",
        )?;
        let projects = statement
            .query_map([], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_name: row.get(2)?,
                    last_used_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(projects)
    }

    pub fn get_settings(&self) -> Result<Settings, StorageError> {
        self.connection
            .execute("INSERT OR IGNORE INTO settings(id) VALUES (1)", [])?;
        self.connection.query_row("SELECT locale, theme, auto_lock_minutes, clipboard_timeout_seconds, require_auth_before_export, require_auth_before_reveal FROM settings WHERE id = 1", [], |row| Ok(Settings { locale: row.get(0)?, theme: row.get(1)?, auto_lock_minutes: row.get(2)?, clipboard_timeout_seconds: row.get(3)?, require_auth_before_export: row.get::<_, i64>(4)? != 0, require_auth_before_reveal: row.get::<_, i64>(5)? != 0 })).map_err(Into::into)
    }

    pub fn update_settings(&self, settings: &Settings) -> Result<(), StorageError> {
        self.connection.execute("INSERT INTO settings(id, locale, theme, auto_lock_minutes, clipboard_timeout_seconds, require_auth_before_export, require_auth_before_reveal) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET locale = excluded.locale, theme = excluded.theme, auto_lock_minutes = excluded.auto_lock_minutes, clipboard_timeout_seconds = excluded.clipboard_timeout_seconds, require_auth_before_export = excluded.require_auth_before_export, require_auth_before_reveal = excluded.require_auth_before_reveal", params![settings.locale, settings.theme, settings.auto_lock_minutes, settings.clipboard_timeout_seconds, settings.require_auth_before_export as i64, settings.require_auth_before_reveal as i64])?;
        Ok(())
    }
}

fn metadata_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SecretMetadata> {
    let tags_json: String = row.get(5)?;
    let tags = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(SecretMetadata {
        id: row.get(0)?,
        name: row.get(1)?,
        provider_id: row.get(2)?,
        env_key: row.get(3)?,
        description: row.get(4)?,
        tags,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
