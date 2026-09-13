# Security Policy

SecretHub is local-first and defaults to no account, no cloud sync and no telemetry. Secret values are designed to be encrypted before storage and are not part of logs, audit events, crash reports or AI-facing metadata.

Do not report a real credential in an issue or chat. Revoke any credential that may have been exposed and report the location privately to the project maintainer.

V1 security acceptance includes encrypted SQLite payloads, metadata-only UI commands, redacted logs/audit, path-safe atomic export, Git tracked `.env` protection and encrypted backups.
