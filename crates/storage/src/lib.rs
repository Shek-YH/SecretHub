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
    pub value_type: String,
    pub category: String,
    pub scope: String,
    pub favorite: bool,
    pub archived: bool,
    pub model_id: String,
    pub model_env_key: String,
    pub endpoint_url: String,
    pub endpoint_env_key: String,
    /// Unix seconds of the last model-list sync attempt; 0 means never.
    pub models_synced_at: i64,
    /// `""` (never), `ok`, `failed` or `unsupported`.
    pub models_sync_status: String,
    /// Unix seconds of the last key validation; 0 means never.
    pub validated_at: i64,
    /// Low-sensitivity plaintext line for lists (website host, bank and last digits, env name).
    pub subtitle: String,
    /// Login items only: exposed to AI/exports as `{env_key}_USERNAME` and `{env_key}_PASSWORD`.
    pub dev_use: bool,
    /// Config groups only: whether AI may read the values of the plain (non-secret) variables.
    pub ai_values: bool,
}

/// Item types whose encrypted payload is a JSON document (typed items and config groups).
pub const JSON_ITEM_TYPES: [&str; 4] = ["login", "bank_card", "note", "env_group"];

/// One variable of a config group. Secret variables keep their value in the encrypted payload,
/// so `plain_value` is always empty for them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupVariableRow {
    pub name: String,
    pub secret: bool,
    pub plain_value: String,
}

impl SecretMetadata {
    /// Whether the payload is a JSON field document rather than a raw string.
    pub fn is_json_item(&self) -> bool {
        JSON_ITEM_TYPES.contains(&self.value_type.as_str())
    }

    /// Typed-items design section 6. The MCP server applies the same rule in SQL.
    pub fn ai_visible(&self) -> bool {
        match self.value_type.as_str() {
            "api_key" | "dev_secret" | "token" | "env_group" => true,
            "url" | "password" | "text" => !self.env_key.is_empty(),
            "login" => self.dev_use,
            _ => false,
        }
    }
}

/// One synced model of one key, with any capability override already applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModelRow {
    pub model_id: String,
    pub display_name: String,
    pub input: Vec<String>,
    pub output: Vec<String>,
    pub capabilities: Vec<String>,
    /// provider|catalog|heuristic|unknown, or the override source (user|ai).
    pub capability_source: String,
    pub synced_at: i64,
}

/// One library file (design M4). Media is not secret: no encryption, readable while locked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaAsset {
    pub id: String,
    /// image | video | audio | text | other
    pub kind: String,
    /// Original file name (display only).
    pub filename: String,
    /// Absolute path of the copy inside the media directory.
    pub stored_path: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub description: String,
    pub tags: Vec<String>,
    /// user | ai
    pub source: String,
    pub created_at: i64,
}

const METADATA_COLUMNS: &str = "id, name, provider_id, env_key, description, tags_json, status, created_at, updated_at, value_type, category, scope, favorite, archived, model_id, model_env_key, endpoint_url, endpoint_env_key, models_synced_at, models_sync_status, validated_at, subtitle, dev_use, ai_values";

const MEDIA_COLUMNS: &str = "id, kind, filename, stored_path, sha256, size_bytes, mime, width, height, duration_ms, description, tags_json, source, created_at";

/// A non-chat capability model chosen for a key (the chat model stays in `secrets.model_id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityModelRow {
    pub capability: String,
    pub model_id: String,
    /// Environment variable name for manual export; empty means "do not export".
    pub env_key: String,
}

/// Capabilities that can have a dedicated model besides the chat model, in display order.
pub const CAPABILITY_MODEL_ORDER: [&str; 9] = [
    "code",
    "reasoning",
    "vision",
    "image_gen",
    "video_gen",
    "tts",
    "asr",
    "embedding",
    "rerank",
];

/// Default environment variable name for a capability model row.
pub fn default_capability_env_key(capability: &str) -> Option<&'static str> {
    Some(match capability {
        "code" => "CODE_MODEL",
        "reasoning" => "REASONING_MODEL",
        "vision" => "VISION_MODEL",
        "image_gen" => "IMAGE_MODEL",
        "video_gen" => "VIDEO_MODEL",
        "tts" => "TTS_MODEL",
        "asr" => "ASR_MODEL",
        "embedding" => "EMBEDDING_MODEL",
        "rerank" => "RERANK_MODEL",
        _ => return None,
    })
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

pub struct SecretMetadataUpdate {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretAttributes {
    pub value_type: String,
    pub category: String,
    pub scope: String,
    pub favorite: bool,
    pub archived: bool,
    pub model_id: String,
    pub model_env_key: String,
    pub endpoint_url: String,
    pub endpoint_env_key: String,
    pub subtitle: String,
    pub dev_use: bool,
    pub ai_values: bool,
}

impl Default for SecretAttributes {
    fn default() -> Self {
        Self {
            value_type: "api_key".into(),
            category: "other".into(),
            scope: "global".into(),
            favorite: false,
            archived: false,
            model_id: String::new(),
            model_env_key: String::new(),
            endpoint_url: String::new(),
            endpoint_env_key: String::new(),
            subtitle: String::new(),
            dev_use: false,
            ai_values: true,
        }
    }
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
    /// Config group variable subsets by secret id; a group without an entry means every variable.
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, Vec<String>>,
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

/// A row still using an old value type that the typed-items migration has to convert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyItem {
    pub id: String,
    pub value_type: String,
    pub env_key: String,
}

/// One converted row; `payload` is set when the encrypted content is rewritten too.
pub struct ItemMigration {
    pub id: String,
    pub value_type: String,
    pub category: Option<String>,
    pub subtitle: String,
    pub payload: Option<EncryptedParts>,
}

/// One payload row re-encrypted for a new master key.
pub struct RekeyedPayload {
    pub secret_id: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

pub type VaultMeta = (Vec<u8>, Vec<u8>);
pub type EncryptedParts = (Vec<u8>, Vec<u8>);
pub type PayloadRow = (String, Vec<u8>, Vec<u8>);

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
                value_type TEXT NOT NULL DEFAULT 'api_key',
                category TEXT NOT NULL DEFAULT 'other',
                scope TEXT NOT NULL DEFAULT 'global',
                favorite INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                model_id TEXT NOT NULL DEFAULT '',
                model_env_key TEXT NOT NULL DEFAULT '',
                endpoint_url TEXT NOT NULL DEFAULT '',
                endpoint_env_key TEXT NOT NULL DEFAULT '',
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
             CREATE TABLE IF NOT EXISTS group_variables (
                secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                name TEXT NOT NULL,
                secret INTEGER NOT NULL,
                plain_value TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (secret_id, name)
             );
             CREATE TABLE IF NOT EXISTS provider_models (
                secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                model_id TEXT NOT NULL,
                display_name TEXT NOT NULL DEFAULT '',
                input_json TEXT NOT NULL DEFAULT '[]',
                output_json TEXT NOT NULL DEFAULT '[]',
                capabilities_json TEXT NOT NULL DEFAULT '[]',
                capability_source TEXT NOT NULL DEFAULT 'unknown',
                synced_at INTEGER NOT NULL,
                PRIMARY KEY (secret_id, model_id)
             );
             CREATE TABLE IF NOT EXISTS secret_capability_models (
                secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                capability TEXT NOT NULL,
                model_id TEXT NOT NULL,
                env_key TEXT NOT NULL,
                PRIMARY KEY (secret_id, capability)
             );
             CREATE TABLE IF NOT EXISTS media_assets (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                filename TEXT NOT NULL,
                stored_path TEXT NOT NULL,
                sha256 TEXT NOT NULL UNIQUE,
                size_bytes INTEGER NOT NULL,
                mime TEXT NOT NULL DEFAULT '',
                width INTEGER,
                height INTEGER,
                duration_ms INTEGER,
                description TEXT NOT NULL DEFAULT '',
                tags_json TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL DEFAULT 'user',
                created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS model_capability_overrides (
                provider_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                capabilities_json TEXT NOT NULL,
                source TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (provider_id, model_id)
             );
             CREATE TABLE IF NOT EXISTS tags (
                name TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL
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
        add_column_if_missing(
            &connection,
            "secrets",
            "value_type",
            "TEXT NOT NULL DEFAULT 'api_key'",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "category",
            "TEXT NOT NULL DEFAULT 'other'",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "scope",
            "TEXT NOT NULL DEFAULT 'global'",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "favorite",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "model_id",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "endpoint_url",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "endpoint_env_key",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "model_env_key",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "archived",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "models_synced_at",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "models_sync_status",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "validated_at",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "subtitle",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "dev_use",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        add_column_if_missing(
            &connection,
            "secrets",
            "ai_values",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        add_column_if_missing(
            &connection,
            "profile_secrets",
            "variables_json",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &connection,
            "settings",
            "catalog_auto_update",
            "INTEGER NOT NULL DEFAULT 1",
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
        self.create_secret_with_attributes(secret, SecretAttributes::default())
    }

    pub fn create_secret_with_attributes(
        &self,
        secret: NewSecret,
        attributes: SecretAttributes,
    ) -> Result<String, StorageError> {
        let id = format!("secret-{}", uuid::Uuid::new_v4());
        let now = unix_time();
        let tags_json = serde_json::to_string(&secret.tags)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO secrets(id, name, provider_id, env_key, description, tags_json, status, value_type, category, scope, favorite, archived, model_id, model_env_key, endpoint_url, endpoint_env_key, subtitle, dev_use, ai_values, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'unknown', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
            params![id, secret.name, secret.provider_id, secret.env_key, secret.description, tags_json, attributes.value_type, attributes.category, attributes.scope, attributes.favorite as i64, attributes.archived as i64, attributes.model_id, attributes.model_env_key, attributes.endpoint_url, attributes.endpoint_env_key, attributes.subtitle, attributes.dev_use as i64, attributes.ai_values as i64, now],
        )?;
        self.connection.execute(
            "INSERT INTO secret_payloads(secret_id, ciphertext, nonce) VALUES (?1, ?2, ?3)",
            params![id, secret.ciphertext, secret.nonce],
        )?;
        Ok(id)
    }

    pub fn get_metadata(&self, id: &str) -> Result<Option<SecretMetadata>, StorageError> {
        self.connection
            .query_row(
                &format!("SELECT {METADATA_COLUMNS} FROM secrets WHERE id = ?1"),
                params![id],
                metadata_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_metadata(&self) -> Result<Vec<SecretMetadata>, StorageError> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {METADATA_COLUMNS} FROM secrets ORDER BY updated_at DESC"
        ))?;
        let rows = statement
            .query_map([], metadata_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn search_metadata(&self, query: &str) -> Result<Vec<SecretMetadata>, StorageError> {
        let pattern = format!("%{}%", query);
        let mut statement = self.connection.prepare(&format!("SELECT {METADATA_COLUMNS} FROM secrets WHERE name LIKE ?1 COLLATE NOCASE OR provider_id LIKE ?1 COLLATE NOCASE OR env_key LIKE ?1 COLLATE NOCASE OR description LIKE ?1 COLLATE NOCASE OR tags_json LIKE ?1 COLLATE NOCASE OR category LIKE ?1 COLLATE NOCASE OR value_type LIKE ?1 COLLATE NOCASE OR model_id LIKE ?1 COLLATE NOCASE OR endpoint_url LIKE ?1 COLLATE NOCASE OR subtitle LIKE ?1 COLLATE NOCASE ORDER BY updated_at DESC"))?;
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

    /// Creates a config group row, its payload and its variable rows in one transaction.
    pub fn create_group(
        &self,
        secret: NewSecret,
        attributes: SecretAttributes,
        variables: &[GroupVariableRow],
    ) -> Result<String, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let id = self.create_secret_with_attributes(secret, attributes)?;
        self.write_group_variables(&id, variables)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Rewrites a config group (row, payload, variables) in one transaction.
    pub fn update_group(
        &self,
        id: &str,
        secret: UpdatedSecret,
        attributes: SecretAttributes,
        variables: &[GroupVariableRow],
    ) -> Result<bool, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        if !self.update_secret_with_attributes(id, secret, attributes)? {
            return Ok(false);
        }
        self.write_group_variables(id, variables)?;
        transaction.commit()?;
        Ok(true)
    }

    fn write_group_variables(
        &self,
        secret_id: &str,
        variables: &[GroupVariableRow],
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM group_variables WHERE secret_id = ?1",
            params![secret_id],
        )?;
        let mut insert = self.connection.prepare(
            "INSERT INTO group_variables(secret_id, position, name, secret, plain_value) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (position, variable) in variables.iter().enumerate() {
            insert.execute(params![
                secret_id,
                position as i64,
                variable.name,
                variable.secret as i64,
                variable.plain_value
            ])?;
        }
        Ok(())
    }

    /// Variables of one config group in display order.
    pub fn list_group_variables(
        &self,
        secret_id: &str,
    ) -> Result<Vec<GroupVariableRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT name, secret, plain_value FROM group_variables WHERE secret_id = ?1 ORDER BY position",
        )?;
        let rows = statement
            .query_map(params![secret_id], |row| {
                Ok(GroupVariableRow {
                    name: row.get(0)?,
                    secret: row.get::<_, i64>(1)? != 0,
                    plain_value: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_secret(&self, id: &str, secret: UpdatedSecret) -> Result<bool, StorageError> {
        self.update_secret_with_attributes(id, secret, SecretAttributes::default())
    }

    pub fn update_secret_with_attributes(
        &self,
        id: &str,
        secret: UpdatedSecret,
        attributes: SecretAttributes,
    ) -> Result<bool, StorageError> {
        let now = unix_time();
        let tags_json = serde_json::to_string(&secret.tags)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        let updated = self.connection.execute("UPDATE secrets SET name = ?1, provider_id = ?2, env_key = ?3, description = ?4, tags_json = ?5, value_type = ?6, category = ?7, scope = ?8, favorite = ?9, archived = ?10, model_id = ?11, model_env_key = ?12, endpoint_url = ?13, endpoint_env_key = ?14, subtitle = ?15, dev_use = ?16, ai_values = ?17, updated_at = ?18 WHERE id = ?19", params![secret.name, secret.provider_id, secret.env_key, secret.description, tags_json, attributes.value_type, attributes.category, attributes.scope, attributes.favorite as i64, attributes.archived as i64, attributes.model_id, attributes.model_env_key, attributes.endpoint_url, attributes.endpoint_env_key, attributes.subtitle, attributes.dev_use as i64, attributes.ai_values as i64, now, id])?;
        if updated == 0 {
            return Ok(false);
        }
        self.connection.execute(
            "UPDATE secret_payloads SET ciphertext = ?1, nonce = ?2 WHERE secret_id = ?3",
            params![secret.ciphertext, secret.nonce, id],
        )?;
        Ok(true)
    }

    pub fn update_metadata_with_attributes(
        &self,
        id: &str,
        metadata: SecretMetadataUpdate,
        attributes: SecretAttributes,
    ) -> Result<bool, StorageError> {
        let now = unix_time();
        let tags_json = serde_json::to_string(&metadata.tags)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        Ok(self.connection.execute(
            "UPDATE secrets SET name = ?1, provider_id = ?2, env_key = ?3, description = ?4, tags_json = ?5, value_type = ?6, category = ?7, scope = ?8, favorite = ?9, archived = ?10, model_id = ?11, model_env_key = ?12, endpoint_url = ?13, endpoint_env_key = ?14, subtitle = ?15, dev_use = ?16, ai_values = ?17, updated_at = ?18 WHERE id = ?19",
            params![metadata.name, metadata.provider_id, metadata.env_key, metadata.description, tags_json, attributes.value_type, attributes.category, attributes.scope, attributes.favorite as i64, attributes.archived as i64, attributes.model_id, attributes.model_env_key, attributes.endpoint_url, attributes.endpoint_env_key, attributes.subtitle, attributes.dev_use as i64, attributes.ai_values as i64, now, id],
        )? > 0)
    }

    /// Replaces all synced models of one key and records the sync outcome atomically.
    pub fn replace_provider_models(
        &self,
        secret_id: &str,
        rows: &[ProviderModelRow],
        synced_at: i64,
        status: &str,
    ) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM provider_models WHERE secret_id = ?1",
            params![secret_id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT OR REPLACE INTO provider_models(secret_id, model_id, display_name, input_json, output_json, capabilities_json, capability_source, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for row in rows {
                insert.execute(params![
                    secret_id,
                    row.model_id,
                    row.display_name,
                    to_json(&row.input)?,
                    to_json(&row.output)?,
                    to_json(&row.capabilities)?,
                    row.capability_source,
                    synced_at,
                ])?;
            }
        }
        transaction.execute(
            "UPDATE secrets SET models_synced_at = ?1, models_sync_status = ?2 WHERE id = ?3",
            params![synced_at, status, secret_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Records a failed/unsupported sync without touching stored model rows.
    pub fn set_models_sync_status(
        &self,
        secret_id: &str,
        status: &str,
        at: i64,
    ) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "UPDATE secrets SET models_synced_at = ?1, models_sync_status = ?2 WHERE id = ?3",
            params![at, status, secret_id],
        )? > 0)
    }

    /// Lists a key's synced models; an override for (provider, model) wins over inferred tags.
    pub fn list_provider_models(
        &self,
        secret_id: &str,
    ) -> Result<Vec<ProviderModelRow>, StorageError> {
        type Raw = (String, String, String, String, String, String, i64);
        let mut statement = self.connection.prepare(
            "SELECT m.model_id, m.display_name, m.input_json, m.output_json,
                    COALESCE(o.capabilities_json, m.capabilities_json),
                    COALESCE(o.source, m.capability_source), m.synced_at
             FROM provider_models m
             JOIN secrets s ON s.id = m.secret_id
             LEFT JOIN model_capability_overrides o
               ON o.provider_id = s.provider_id AND o.model_id = m.model_id
             WHERE m.secret_id = ?1
             ORDER BY m.model_id",
        )?;
        let raw = statement
            .query_map(params![secret_id], |row| -> rusqlite::Result<Raw> {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(model_id, display_name, input, output, caps, source, synced_at)| {
                    Ok(ProviderModelRow {
                        model_id,
                        display_name,
                        input: from_json(&input)?,
                        output: from_json(&output)?,
                        capabilities: from_json(&caps)?,
                        capability_source: source,
                        synced_at,
                    })
                },
            )
            .collect()
    }

    pub fn insert_media(&self, asset: &MediaAsset) -> Result<(), StorageError> {
        self.connection.execute(
            &format!("INSERT INTO media_assets({MEDIA_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"),
            params![
                asset.id,
                asset.kind,
                asset.filename,
                asset.stored_path,
                asset.sha256,
                asset.size_bytes,
                asset.mime,
                asset.width,
                asset.height,
                asset.duration_ms,
                asset.description,
                to_json(&asset.tags)?,
                asset.source,
                asset.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_media(&self, id: &str) -> Result<Option<MediaAsset>, StorageError> {
        self.query_media("WHERE id = ?1", params![id])
            .map(|mut rows| rows.pop())
    }

    pub fn find_media_by_sha(&self, sha256: &str) -> Result<Option<MediaAsset>, StorageError> {
        self.query_media("WHERE sha256 = ?1", params![sha256])
            .map(|mut rows| rows.pop())
    }

    /// Every asset, newest first.
    pub fn list_media(&self) -> Result<Vec<MediaAsset>, StorageError> {
        self.query_media("", params![])
    }

    fn query_media(
        &self,
        condition: &str,
        arguments: impl rusqlite::Params,
    ) -> Result<Vec<MediaAsset>, StorageError> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {MEDIA_COLUMNS} FROM media_assets {condition} ORDER BY created_at DESC, id"
        ))?;
        let raw = statement
            .query_map(arguments, |row| {
                Ok((
                    MediaAsset {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        filename: row.get(2)?,
                        stored_path: row.get(3)?,
                        sha256: row.get(4)?,
                        size_bytes: row.get(5)?,
                        mime: row.get(6)?,
                        width: row.get(7)?,
                        height: row.get(8)?,
                        duration_ms: row.get(9)?,
                        description: row.get(10)?,
                        tags: Vec::new(),
                        source: row.get(12)?,
                        created_at: row.get(13)?,
                    },
                    row.get::<_, String>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(mut asset, tags)| {
                asset.tags = from_json(&tags)?;
                Ok(asset)
            })
            .collect()
    }

    pub fn update_media_meta(
        &self,
        id: &str,
        description: &str,
        tags: &[String],
    ) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "UPDATE media_assets SET description = ?1, tags_json = ?2 WHERE id = ?3",
            params![description, to_json(tags)?, id],
        )? > 0)
    }

    /// Removes the row and returns it so the caller can delete the stored copy.
    pub fn delete_media(&self, id: &str) -> Result<Option<MediaAsset>, StorageError> {
        let existing = self.get_media(id)?;
        if existing.is_some() {
            self.connection
                .execute("DELETE FROM media_assets WHERE id = ?1", params![id])?;
        }
        Ok(existing)
    }

    /// A key's per-capability model choices in display order.
    pub fn list_capability_models(
        &self,
        secret_id: &str,
    ) -> Result<Vec<CapabilityModelRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT capability, model_id, env_key FROM secret_capability_models WHERE secret_id = ?1",
        )?;
        let mut rows = statement
            .query_map(params![secret_id], |row| {
                Ok(CapabilityModelRow {
                    capability: row.get(0)?,
                    model_id: row.get(1)?,
                    env_key: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by_key(|row| {
            CAPABILITY_MODEL_ORDER
                .iter()
                .position(|item| *item == row.capability)
                .unwrap_or(usize::MAX)
        });
        Ok(rows)
    }

    /// Replaces all capability model rows of one key atomically. Validation is the caller's job.
    pub fn replace_capability_models(
        &self,
        secret_id: &str,
        rows: &[CapabilityModelRow],
    ) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM secret_capability_models WHERE secret_id = ?1",
            params![secret_id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO secret_capability_models(secret_id, capability, model_id, env_key) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for row in rows {
                insert.execute(params![
                    secret_id,
                    row.capability,
                    row.model_id,
                    row.env_key
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Stores a confirmed capability override; syncing never touches this table.
    pub fn upsert_capability_override(
        &self,
        provider_id: &str,
        model_id: &str,
        capabilities: &[String],
        source: &str,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO model_capability_overrides(provider_id, model_id, capabilities_json, source, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider_id, model_id) DO UPDATE SET capabilities_json = excluded.capabilities_json, source = excluded.source, updated_at = excluded.updated_at",
            params![provider_id, model_id, to_json(capabilities)?, source, unix_time()],
        )?;
        Ok(())
    }

    /// Rows that still carry a pre-typed-items value type.
    pub fn list_legacy_items(&self) -> Result<Vec<LegacyItem>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, value_type, env_key FROM secrets WHERE value_type IN ('token', 'url', 'password', 'text') ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(LegacyItem {
                    id: row.get(0)?,
                    value_type: row.get(1)?,
                    env_key: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Applies all conversions in one transaction; any failure leaves every row untouched.
    pub fn apply_item_migrations(&self, items: &[ItemMigration]) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        for item in items {
            let changed = match &item.category {
                Some(category) => transaction.execute(
                    "UPDATE secrets SET value_type = ?1, category = ?2, subtitle = ?3 WHERE id = ?4",
                    params![item.value_type, category, item.subtitle, item.id],
                )?,
                None => transaction.execute(
                    "UPDATE secrets SET value_type = ?1, subtitle = ?2 WHERE id = ?3",
                    params![item.value_type, item.subtitle, item.id],
                )?,
            };
            if changed != 1 {
                return Err(StorageError::InvalidData("migrated row is missing".into()));
            }
            if let Some((ciphertext, nonce)) = &item.payload {
                transaction.execute(
                    "UPDATE secret_payloads SET ciphertext = ?1, nonce = ?2 WHERE secret_id = ?3",
                    params![ciphertext, nonce, item.id],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Every encrypted payload as (secret id, ciphertext, nonce).
    pub fn list_payloads(&self) -> Result<Vec<PayloadRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT secret_id, ciphertext, nonce FROM secret_payloads ORDER BY secret_id",
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Swaps every payload and the vault salt/verifier in one transaction (master password
    /// change). `payloads` must cover each stored payload exactly; otherwise nothing changes.
    pub fn rekey(
        &self,
        salt: &[u8],
        verifier: &[u8],
        payloads: &[RekeyedPayload],
    ) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let stored: i64 =
            transaction.query_row("SELECT COUNT(*) FROM secret_payloads", [], |row| row.get(0))?;
        if stored != payloads.len() as i64 {
            return Err(StorageError::InvalidData(
                "payload set changed during rekey".into(),
            ));
        }
        for payload in payloads {
            let changed = transaction.execute(
                "UPDATE secret_payloads SET ciphertext = ?1, nonce = ?2 WHERE secret_id = ?3",
                params![payload.ciphertext, payload.nonce, payload.secret_id],
            )?;
            if changed != 1 {
                return Err(StorageError::InvalidData("payload row is missing".into()));
            }
        }
        let changed = transaction.execute(
            "UPDATE vault_meta SET salt = ?1, verifier = ?2 WHERE id = 1",
            params![salt, verifier],
        )?;
        if changed != 1 {
            return Err(StorageError::InvalidData("vault is not initialized".into()));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn update_status(&self, id: &str, status: &str) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "UPDATE secrets SET status = ?1, validated_at = ?2 WHERE id = ?3",
            params![status, unix_time(), id],
        )? > 0)
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
            // An empty column means "every variable".
            let subset = match profile.variables.get(secret_id) {
                Some(names) => to_json(names)?,
                None => String::new(),
            };
            self.connection.execute(
                "INSERT INTO profile_secrets(profile_id, secret_id, position, variables_json) VALUES (?1, ?2, ?3, ?4)",
                params![profile.id, secret_id, position as i64, subset],
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
            let members = self
                .connection
                .prepare(
                    "SELECT secret_id, variables_json FROM profile_secrets WHERE profile_id = ?1 ORDER BY position",
                )?
                .query_map(params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut ids = Vec::new();
            let mut variables = std::collections::BTreeMap::new();
            for (secret_id, subset) in members {
                if !subset.is_empty() {
                    variables.insert(secret_id.clone(), from_json(&subset)?);
                }
                ids.push(secret_id);
            }
            profiles.push(StoredProfile {
                id,
                name,
                description,
                secret_ids: ids,
                variables,
                created_at,
                updated_at,
            });
        }
        Ok(profiles)
    }

    pub fn delete_profile(&self, id: &str) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?
            > 0)
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

    /// Tag library: stored tags unioned with tags used on secrets, deduped case-insensitively.
    pub fn list_tags(&self) -> Result<Vec<String>, StorageError> {
        let mut names: Vec<String> = self
            .connection
            .prepare("SELECT name FROM tags")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        for secret in self.list_metadata()? {
            names.extend(secret.tags);
        }
        for asset in self.list_media()? {
            names.extend(asset.tags);
        }
        let mut seen = std::collections::HashSet::new();
        names.retain(|name| !name.trim().is_empty() && seen.insert(name.trim().to_lowercase()));
        names.sort_by_key(|name| (name.to_lowercase(), name.clone()));
        Ok(names)
    }

    /// Adds a tag to the library; a case-insensitive duplicate returns the existing spelling.
    pub fn create_tag(&self, name: &str) -> Result<String, StorageError> {
        let name = name.trim();
        let length = name.chars().count();
        if length == 0 || length > 32 || name.contains(',') || name.chars().any(char::is_control) {
            return Err(StorageError::InvalidData("invalid tag name".to_owned()));
        }
        if let Some(existing) = self
            .list_tags()?
            .into_iter()
            .find(|tag| tag.to_lowercase() == name.to_lowercase())
        {
            return Ok(existing);
        }
        self.connection.execute(
            "INSERT INTO tags(name, created_at) VALUES (?1, ?2)",
            params![name, unix_time()],
        )?;
        Ok(name.to_owned())
    }

    /// Removes a tag from the library only; secrets keep their tags.
    pub fn delete_tag(&self, name: &str) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "DELETE FROM tags WHERE lower(name) = lower(?1)",
            params![name.trim()],
        )? > 0)
    }

    pub fn get_settings(&self) -> Result<Settings, StorageError> {
        self.connection
            .execute("INSERT OR IGNORE INTO settings(id) VALUES (1)", [])?;
        self.connection.query_row("SELECT locale, theme, auto_lock_minutes, clipboard_timeout_seconds, require_auth_before_export, require_auth_before_reveal FROM settings WHERE id = 1", [], |row| Ok(Settings { locale: row.get(0)?, theme: row.get(1)?, auto_lock_minutes: row.get(2)?, clipboard_timeout_seconds: row.get(3)?, require_auth_before_export: row.get::<_, i64>(4)? != 0, require_auth_before_reveal: row.get::<_, i64>(5)? != 0 })).map_err(Into::into)
    }

    /// Whether the remote model catalog is fetched automatically (default on).
    pub fn catalog_auto_update(&self) -> Result<bool, StorageError> {
        self.connection
            .execute("INSERT OR IGNORE INTO settings(id) VALUES (1)", [])?;
        Ok(self.connection.query_row(
            "SELECT catalog_auto_update FROM settings WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }

    pub fn set_catalog_auto_update(&self, enabled: bool) -> Result<(), StorageError> {
        self.connection
            .execute("INSERT OR IGNORE INTO settings(id) VALUES (1)", [])?;
        self.connection.execute(
            "UPDATE settings SET catalog_auto_update = ?1 WHERE id = 1",
            params![enabled as i64],
        )?;
        Ok(())
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
        value_type: row.get(9)?,
        category: row.get(10)?,
        scope: row.get(11)?,
        favorite: row.get::<_, i64>(12)? != 0,
        archived: row.get::<_, i64>(13)? != 0,
        model_id: row.get(14)?,
        model_env_key: row.get(15)?,
        endpoint_url: row.get(16)?,
        endpoint_env_key: row.get(17)?,
        models_synced_at: row.get(18)?,
        models_sync_status: row.get(19)?,
        validated_at: row.get(20)?,
        subtitle: row.get(21)?,
        dev_use: row.get::<_, i64>(22)? != 0,
        ai_values: row.get::<_, i64>(23)? != 0,
    })
}

fn to_json(values: &[String]) -> Result<String, StorageError> {
    serde_json::to_string(values).map_err(|error| StorageError::InvalidData(error.to_string()))
}

fn from_json(text: &str) -> Result<Vec<String>, StorageError> {
    serde_json::from_str(text).map_err(|error| StorageError::InvalidData(error.to_string()))
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }
    connection.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    ))?;
    Ok(())
}

fn unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
