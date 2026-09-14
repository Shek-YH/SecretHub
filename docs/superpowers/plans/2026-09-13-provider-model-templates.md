# Provider Model Templates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Add Secret open a provider/model template picker that pre-fills provider fields, exports the selected model through metadata, allows optional environment keys, and opens the provider's API-key page.

**Architecture:** Keep provider/model templates as local, value-free TypeScript data. Persist selected model metadata beside the encrypted payload (`model_id`, `model_env_key`), and let the existing Rust exporter emit the API key plus model environment entry. External links are fixed template URLs; no user secret is sent to the browser or network.

**Tech Stack:** React/TypeScript, Tauri 2/Rust, SQLite, existing encrypted vault/exporter.

---

### Task 1: Persist selected model metadata

**Files:** `crates/storage/src/lib.rs`, `crates/core/src/lib.rs`, `apps/desktop/src-tauri/src/commands.rs`, related Rust tests.

- [ ] Add value-free `model_id` and `model_env_key` metadata with migration defaults.
- [ ] Add a failing restart test proving metadata and encrypted value survive together.
- [ ] Extend exporter selection to emit the model env entry without exposing the API key in previews.

### Task 2: Add provider/model catalog

**Files:** `apps/desktop/src/providers.ts`, `apps/desktop/src/providers.test.ts`.

- [ ] Add mainstream China/global providers, model IDs, API-key URLs and default env keys.
- [ ] Keep a Custom Provider/Custom Model path with no guessed endpoint.
- [ ] Test catalog uniqueness, URL safety and model-to-env mapping.

### Task 3: Replace Add Secret with template-first form

**Files:** `apps/desktop/src/App.tsx`, `apps/desktop/src/lib/backend.ts`, `apps/desktop/src/styles.css`, `apps/desktop/src/App.test.tsx`.

- [ ] Add provider selection before the form and model selection after provider selection.
- [ ] Make Env Key optional; only emit it when non-empty and valid.
- [ ] Add API-key website and URL-value open actions without storing plaintext in localStorage.

### Task 4: Fill safe synthetic fixtures and package

**Files:** `tests/e2e/golden_path.py`, evidence and ledger files.

- [ ] Exercise OpenAI, DeepSeek, Gemini and custom provider templates using synthetic values only.
- [ ] Run frontend/Rust/security/E2E gates and rebuild MSI/NSIS.
- [ ] Record manual acceptance limits separately from automated evidence.
