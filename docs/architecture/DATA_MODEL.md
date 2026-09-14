# SecretHub Data Model

## Vault

- `vault_meta(id, schema_version, kdf_algorithm, kdf_salt, verifier, created_at, updated_at)`
- `settings(id, locale, theme, auto_lock_minutes, clipboard_timeout_seconds, require_auth_before_export, require_auth_before_reveal)`

## Secret

- `secrets(id, name, provider_id, env_key, description, tags_json, status, value_type, category, scope, favorite, archived, model_id, model_env_key, endpoint_url, endpoint_env_key, created_at, updated_at, last_used_at)`
- `secret_payloads(secret_id, ciphertext, nonce, algorithm, version)`

The payload table is intentionally separated so metadata search never touches plaintext.

`model_id`, `model_env_key`, `endpoint_url` and `endpoint_env_key` are value-free metadata. When selected for export, the existing encrypted payload supplies the API-key value and the model/request metadata supplies separate project environment entries.

## Profile and project

- `profiles(id, name, description, created_at, updated_at)`
- `profile_secrets(profile_id, secret_id, position)`
- `projects(id, path, display_name, last_used_at)`

Profiles store Secret IDs, not copied values. Profile application resolves the current encrypted payloads at export time.

## Audit

- `audit_events(id, operation, secret_id, project_id, value_hash, result, created_at, metadata_json)`

Audit metadata is allowlisted and redacted; no value or payload is stored.

Project export is intentionally a separate projection from the vault record: it emits Env key/value pairs and optional remark comments only. Management metadata remains local to SecretHub.
