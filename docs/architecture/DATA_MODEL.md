# SecretHub Data Model

## Vault

- `vault_meta(id, schema_version, kdf_algorithm, kdf_salt, verifier, created_at, updated_at)`
- `settings(id, locale, theme, auto_lock_minutes, clipboard_timeout_seconds, require_auth_before_export, require_auth_before_reveal)`

## Secret

- `secrets(id, name, provider_id, env_key, description, tags_json, status, value_type, category, scope, favorite, archived, created_at, updated_at, last_used_at)`
- `secret_payloads(secret_id, ciphertext, nonce, algorithm, version)`

The payload table is intentionally separated so metadata search never touches plaintext.

## Profile and project

- `profiles(id, name, description, created_at, updated_at)`
- `profile_secrets(profile_id, secret_id, position)`
- `projects(id, path, display_name, last_used_at)`

## Audit

- `audit_events(id, operation, secret_id, project_id, value_hash, result, created_at, metadata_json)`

Audit metadata is allowlisted and redacted; no value or payload is stored.
