# SecretHub V1 Task Ledger

The machine-readable ledger in `.ai-ledger/` is authoritative for execution state. This file is a human-readable phase map kept in sync at phase boundaries.

| ID | Phase | Goal | Status | Evidence |
|---|---|---|---|---|
| R01 | Architecture | decisions, threat model, data model, task decomposition | IN_PROGRESS | design/governance docs |
| R02 | Core Security | KDF, AES-GCM, zeroization, redaction | NOT_STARTED | pending Rust tests |
| R03 | Storage | SQLite schema, migrations, repository | NOT_STARTED | pending integration tests |
| R04 | Vault | CRUD, mask/reveal/copy, lock lifecycle | NOT_STARTED | pending vertical slice |
| R05 | Desktop | shell, list, detail, add/edit, search, zh/en switch | NOT_STARTED | pending UI tests |
| R06 | Export | selection, folder picker, preview, dotenv, conflict, gitignore | NOT_STARTED | pending E2E |
| R07 | Profile | create/edit/apply, project history | NOT_STARTED | pending integration |
| R08 | Provider | registry, detection, fixed validators | NOT_STARTED | pending provider tests |
| R09 | QA | unit/integration/security/E2E/release artifacts | NOT_STARTED | pending full gate |
