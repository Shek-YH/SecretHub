# SecretHub Architecture

## Runtime

```text
React/TypeScript UI
        │ Tauri IPC (metadata-first)
        ▼
Rust Core / Commands
 ├─ crypto (Argon2id + AES-GCM + zeroize)
 ├─ storage (SQLite + migrations)
 ├─ exporter (dotenv + atomic filesystem writes)
 ├─ providers (local registry + fixed validators)
 ├─ profiles/projects
 ├─ policy
 └─ redaction/audit
```

Local integrations:

```text
CLI ───────┐
MCP stdio ─┼─> shared encrypted SQLite / Rust Core boundary
Desktop ───┘
```

The MCP process opens SQLite read-only for metadata catalog access. It never decrypts `secret_payloads`; write-capable project preparation is returned as a confirmation-required plan for Desktop.

The optional V3.5 proxy is a separate localhost process. It decrypts a selected value only after an interactive vault unlock, authenticates Agent requests with a 300-second token hash, forwards only to fixed Provider origins, and never returns the provider credential.

## V1 command boundary

- `vault_status`, `unlock`, `lock`: status only; unlock never returns a key.
- `secret_list`, `secret_get_metadata`, `secret_create`, `secret_update`, `secret_delete`.
- `secret_copy`, `secret_reveal`: native-only value handling; no plaintext response to WebView.
- `export_preview`, `export_env`, `backup_export`.
- `profile_list`, `profile_save`, `profile_apply`.
- `settings_get`, `settings_update`, `audit_list`.

## Persistence

The database stores schema version, vault parameters, secret metadata, encrypted payload blobs, profiles, project history, settings and audit events. Secret values are never stored as a separate plaintext column.

## Frontend

The frontend is a dense but readable developer utility: left navigation, searchable secret list, metadata detail pane, selection toolbar, native-feeling controls, and explicit security warnings. `zh-CN` is the initial locale; `en-US` is a complete alternate dictionary.
