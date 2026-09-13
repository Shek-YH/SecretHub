# Third-Party Notices

No third-party source code has been copied into SecretHub at project initialization. Direct unmodified dependencies:

| Dependency | Resolved family | License | Use |
|---|---:|---|---|
| React / React DOM | 19.x | MIT | Desktop UI |
| Vite / TypeScript / Vitest | 7.x / 5.x / 3.x | MIT | Build, typecheck, tests |
| Tauri / Tauri API | 2.x | Apache-2.0 / MIT | Desktop shell and IPC |
| AES-GCM / Argon2 / zeroize | 0.10 / 0.5 / 1.x | Apache-2.0 / MIT | Encryption and memory hygiene |
| rusqlite (bundled SQLite) | 0.32.x | MIT | Local storage |
| dotenvy | 0.15.x | MIT | Dotenv parsing |
| arboard / rfd | 3.x / 0.15.x | MIT | Native clipboard and dialogs |
| serde / serde_json | 1.x | Apache-2.0 / MIT | Serialization |
| Playwright Python | host-provided | Apache-2.0 | Browser E2E verification |

Transitive licenses remain governed by the package lockfiles. A public release must generate a complete dependency license report. `npm audit` is currently clean; `cargo audit` exits 0 but reports seven unmaintained/unsound transitive warnings in cross-platform Tauri build dependencies (`proc-macro-error`, `unic-*`, `glib`). These are tracked for dependency refresh and are not silently treated as zero-risk.

Dependencies will be listed here with version, license, attribution and modification details before a release artifact is produced.
