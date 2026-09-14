# SecretHub V1 Task Ledger

The machine-readable ledger in `.ai-ledger/` is authoritative for execution state. This file is a human-readable phase map kept in sync at phase boundaries.

| ID | Phase | Goal | Status | Evidence |
|---|---|---|---|---|
| R01 | Architecture | decisions, threat model, data model, task decomposition | COMPLETED | design/governance docs |
| R02 | Core Security | KDF, AES-GCM, zeroization, redaction | COMPLETED | R02-R09 evidence |
| R03 | Storage | SQLite schema, migrations, repository | COMPLETED | storage/core tests |
| R04 | Vault | CRUD, mask/reveal/copy, lock lifecycle | COMPLETED | restart/unlock + CRUD tests |
| R05 | Desktop | shell, list, detail, add/edit, search, zh/en switch | COMPLETED | Vitest + Playwright E2E |
| R06 | Export | selection, folder picker, preview, dotenv, conflict, gitignore | COMPLETED | exporter tests + Tauri commands |
| R07 | Profile | create/edit/apply, project history | COMPLETED | profile/storage tests + UI pages |
| R08 | Provider | registry, detection, fixed validators | COMPLETED | registry + HTTPS/SSRF tests |
| R09 | QA | unit/integration/security/E2E/release artifacts | IN_PROGRESS | automated gates pass; native window manual acceptance pending |
| R10 | V2 MCP | metadata catalog, recommendation and confirmation-required plans | COMPLETED | MCP protocol smoke + TypeScript tests |
| R11 | CLI | shared-core status/catalog/hidden-input create | COMPLETED | Rust check/clippy/test/help smoke |
| R12 | Import / AI bridge | `.env`/JSON import and confirmation boundary | COMPLETED | parser/Tauri commands/pending-plan bridge; native manual visual acceptance remains under R09 |
| R13 | Agent Proxy | short-lived token and fixed endpoint policy | WAITING_USER | HTTP forwarding and unauthorized tests pass; real-provider smoke needs local test credential |
| R14 | Classification | type/category/scope/favorite/archived metadata and filters | COMPLETED | storage/core tests + desktop filters |
| R15 | Provider templates | provider-first add flow, model metadata and optional env key | COMPLETED | provider/model tests, E2E, Rust migration and Tauri package pass; real provider/manual acceptance remains separate |
