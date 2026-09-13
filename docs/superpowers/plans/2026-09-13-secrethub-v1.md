# SecretHub V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Windows-first SecretHub V1 described by the PRD, including a secure local vault, project-aware `.env` export, profiles, audit, security hardening, packaging, and a default-Chinese zh/en UI switch.

**Architecture:** Tauri 2 hosts a React/TypeScript desktop UI. Rust owns plaintext handling, crypto, persistence, filesystem export, native clipboard/dialogs, policy and redaction; the UI receives metadata and operation results only. SQLite stores metadata plus AES-GCM encrypted payloads, with Argon2id-derived keys and zeroization.

**Tech Stack:** React 18+, TypeScript, Vite, Tauri 2, Rust stable, SQLite via rusqlite, Argon2, AES-GCM, zeroize, serde, Vitest, Rust tests, Playwright/Tauri E2E.

---

## Task 1: Bootstrap and toolchain

**Files:**
- Create: `package.json`, `apps/desktop/package.json`, `apps/desktop/index.html`, `apps/desktop/src/main.tsx`, `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css`
- Create: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/tauri.conf.json`
- Create: `.gitignore`, `Cargo.toml`, `rust-toolchain.toml`
- Test: `apps/desktop/src/App.test.tsx`

- [ ] Install/verify Rust stable and Tauri prerequisites; run `node --version`, `npm --version`, `cargo --version`, `rustc --version`.
- [ ] Add the smallest Vite/Tauri shell with a test command and no secret-bearing state.
- [ ] Run `npm test -- --run` and `cargo test` from the new workspace; both must exit 0.
- [ ] Run `npm run build` and `cargo check`; both must exit 0.

## Task 2: Security core (R02)

**Files:**
- Create: `crates/crypto/Cargo.toml`, `crates/crypto/src/lib.rs`, `crates/redaction/Cargo.toml`, `crates/redaction/src/lib.rs`
- Create: `tests/security/plaintext_scan.ps1`
- Test: Rust unit tests in each crate

- [ ] Write failing tests for round-trip encryption, wrong-password rejection, tamper rejection, zeroization-compatible ownership, and redaction of `api_key`, `token`, `password`, `authorization`, `cookie`, and `private_key`.
- [ ] Implement Argon2id key derivation, AES-256-GCM payload encryption/decryption, and field-aware redaction without logging values.
- [ ] Run `cargo test -p secrethub-crypto -p secrethub-redaction`; expected all tests pass.
- [ ] Run the plaintext scan against source, tests and generated logs; expected no `TEST_SECRET_123456`.

## Task 3: SQLite storage (R03)

**Files:**
- Create: `crates/storage/Cargo.toml`, `crates/storage/src/lib.rs`, `crates/storage/src/schema.rs`, `crates/storage/src/repository.rs`
- Test: `crates/storage/tests/repository.rs`

- [ ] Write failing repository tests for vault initialization, schema migration, secret CRUD, metadata search, profiles, projects, settings and audit records.
- [ ] Implement parameterized SQLite schema and repository methods; keep encrypted payload separate from searchable metadata.
- [ ] Run `cargo test -p secrethub-storage`; expected all tests pass and database scan finds no plaintext sentinel.

## Task 4: Core vault and policy (R04)

**Files:**
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/vault.rs`, `crates/core/src/policy.rs`, `crates/core/src/models.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Test: `crates/core/tests/vault_flow.rs`

- [ ] Write a failing integration test for first-run master setup, lock, unlock after reopening the database, metadata-only list, update and delete.
- [ ] Implement vault state, KDF parameter storage, encrypted payload persistence, masking, policy decisions and audit events.
- [ ] Add native-only copy/reveal commands: copy writes through the native clipboard; reveal opens a short-lived native dialog and never returns plaintext to WebView.
- [ ] Run `cargo test -p secrethub-core` and the restart/unlock integration test.

## Task 5: Desktop shell and localization (R05)

**Files:**
- Create: `apps/desktop/src/i18n/translations.ts`, `apps/desktop/src/i18n/useLocale.ts`, `apps/desktop/src/components/AppShell.tsx`, `apps/desktop/src/components/SecretList.tsx`, `apps/desktop/src/components/SecretDetail.tsx`, `apps/desktop/src/components/SecretForm.tsx`, `apps/desktop/src/components/SettingsPanel.tsx`
- Modify: `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css`
- Test: `apps/desktop/src/App.test.tsx`, `apps/desktop/src/i18n/translations.test.ts`

- [ ] Write failing UI tests proving default `zh-CN`, complete English dictionary keys, immediate locale toggle, and persistence of locale without secret values in local storage.
- [ ] Implement the dense three-region desktop layout, empty/loading/error states, search, selection toolbar, details, add/edit/delete forms, settings and language switch.
- [ ] Ensure all visible strings come from translation dictionaries; maintain the high-fidelity interactive prototype in the actual UI.
- [ ] Run Vitest plus `npm run build`; inspect the rendered app for the key acceptance states.

## Task 6: Export engine and Git safety (R06)

**Files:**
- Create: `crates/exporter/Cargo.toml`, `crates/exporter/src/lib.rs`, `crates/exporter/src/dotenv.rs`, `crates/exporter/src/git.rs`
- Modify: `crates/core/src/lib.rs`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src/components/SecretList.tsx`
- Test: `crates/exporter/tests/dotenv.rs`, `crates/exporter/tests/safety.rs`

- [ ] Write failing tests for spaces, `#`, quotes, multiline values, URLs, JSON, private keys, Unicode, Windows newlines, merge/conflict preview, tracked `.env`, missing `.gitignore`, and atomic failure.
- [ ] Implement mature escaping rules in the project-owned writer, preview and conflict model, canonical project-folder validation, `.gitignore` check/fix, `.env.example`, and atomic write.
- [ ] Add folder picker and export confirmation UI; block tracked `.env` until remediation confirmation.
- [ ] Run exporter tests, core integration tests, and a real temp-directory export smoke test.

## Task 7: Profiles and projects (R07)

**Files:**
- Create: `crates/profiles/Cargo.toml`, `crates/profiles/src/lib.rs`, `apps/desktop/src/components/ProfilePanel.tsx`
- Modify: `crates/core/src/lib.rs`, `apps/desktop/src/App.tsx`, `apps/desktop/src/i18n/translations.ts`
- Test: `crates/profiles/tests/profile_flow.rs`, `apps/desktop/src/components/ProfilePanel.test.tsx`

- [ ] Write failing tests for profile create/edit/apply, ordering, missing secret handling and recent project history.
- [ ] Implement profile persistence and apply-to-selection behavior with policy/audit coverage.
- [ ] Add profile and project-history UI in both locales; export through the existing safe path.
- [ ] Run profile tests and UI tests.

## Task 8: Provider registry (R08)

**Files:**
- Create: `crates/providers/Cargo.toml`, `crates/providers/src/lib.rs`, `crates/providers/src/registry.rs`, `crates/providers/src/detect.rs`, `crates/providers/src/validators.rs`
- Create: `packages/provider-definitions/providers.json`
- Test: `crates/providers/tests/detection.rs`, `crates/providers/tests/ssrf.rs`

- [ ] Write failing tests for Generic, OpenAI, Anthropic, Gemini, DeepSeek, xAI, OpenRouter, GitHub, Supabase and Cloudflare metadata/prefix detection; unknown values must not trigger network probes.
- [ ] Implement registry and deterministic local detection; keep validator origins fixed and private-network requests rejected.
- [ ] Add provider filter and custom-provider metadata UI; provider validation remains opt-in and synthetic by default.
- [ ] Run provider tests and dependency/license audit.

## Task 9: Hardening, backup, lock and release artifacts (R09)

**Files:**
- Modify: `crates/core/src/vault.rs`, `crates/storage/src/repository.rs`, `apps/desktop/src/App.tsx`, `apps/desktop/src/components/SettingsPanel.tsx`
- Create: `crates/backup/Cargo.toml`, `crates/backup/src/lib.rs`, `tests/integration/golden_path.rs`, `tests/security/security_scan.ps1`, `tests/e2e/golden-path.spec.ts`
- Modify: `README.md`, `SECURITY.md`, `THIRD_PARTY_NOTICES.md`, `docs/governance/TASK_LEDGER.md`

- [ ] Write failing tests for auto-lock, locked-operation denial, encrypted backup, clipboard conditional clearing, concurrent writes and security scans.
- [ ] Implement settings, lock timeout, encrypted backup, audit retention, safe clipboard cleanup and release metadata.
- [ ] Run the full Rust workspace test suite, frontend tests, build, security scans, and Playwright/Tauri E2E where the host can launch the packaged app.
- [ ] Verify the Golden Path: first launch → master password → add → restart → unlock → metadata → select → folder → preview → export `.env` and `.env.example`.
- [ ] Inspect `git diff --check`, `git status`, staged files and secret scans before any local checkpoint.

## Completion gate

- [ ] Every PRD V1 DoD item has an evidence entry in `docs/evidence/`.
- [ ] `zh-CN` is the default and `en-US` is switchable on every implemented page.
- [ ] No real credential or test sentinel is present in source, DB fixtures, logs, snapshots, ledger or Git history.
- [ ] Windows build/install and manual acceptance are recorded separately from automated tests; any unavailable host capability remains explicitly `WAITING_USER`, not silently marked complete.
