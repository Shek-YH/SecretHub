# SecretHub Security Model

## Assets

- Secret values: API keys, tokens, passwords, private keys and related fields.
- Master Password and derived vault key.
- Encrypted vault database and encrypted backup.
- Exported `.env` and clipboard contents.
- Metadata, audit events and project paths.

## Trust boundaries

1. Rust Core is the trusted boundary for plaintext handling.
2. React/WebView is untrusted for secret handling and receives metadata only.
3. SQLite/filesystem is untrusted storage; payloads are encrypted before persistence.
4. Project directories and Git state are external inputs and require canonicalization and policy checks.
5. Provider validation is an outbound network boundary and is opt-in, fixed-domain and SSRF constrained.

## Threats and mitigations

| Threat | Mitigation | Verification |
|---|---|---|
| Database disclosure | Argon2id-derived key + AES-256-GCM encrypted payload | SQLite plaintext scan |
| UI/DevTools exposure | Metadata-only commands; native Reveal/Copy path | UI command contract tests |
| Log/crash leakage | field-aware redaction before log/report emission | log/temp/audit scan |
| `.env` partial write | temp file, flush, atomic rename | failure injection test |
| Git accidental commit | tracked-file fail-closed; `.gitignore` check | integration/E2E |
| Path traversal/junction | canonicalize selected directory and reject unsafe target | security tests |
| SSRF | fixed provider origins, private-IP rejection, timeout/redirect limits | validator tests |
| Clipboard clobber | token/hash and conditional clear | clipboard tests |
| Brute force | Argon2id cost, lockout/backoff policy, no recovery bypass | security tests |

## Non-goals

V1 does not protect a compromised host, keylogger, administrator, malicious WebView runtime, or plaintext intentionally exported by the user. Cloud sync, multi-user access, runtime injection and MCP are outside the V1 trust boundary.

## Secret handling invariant

No test fixture, source file, snapshot, log, crash report, audit record, `.ai-ledger` file or commit may contain a real credential or the sentinel `TEST_SECRET_123456`.
