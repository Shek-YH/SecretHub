# Risk Register

| ID | Risk | Level | Mitigation / evidence | State |
|---|---|---:|---|---|
| RSK-001 | Rust toolchain missing at intake | High | Install official toolchain; verify `cargo test` and Tauri build | Open |
| RSK-002 | WebView can expose plaintext if IPC returns it | Critical | Native-only Reveal/Copy; metadata-only command contract | Open |
| RSK-003 | V1 scope spans desktop, crypto and filesystem | High | Dependency-ordered work items and vertical slice gate | Active |
| RSK-004 | `.env` accidentally tracked by Git | Critical | Fail-closed preflight and integration test | Open |
| RSK-005 | Real provider validation needs private credentials | Medium | Synthetic readiness first; user-local only when required | Deferred |
| RSK-006 | UI prototype drifts from implementation | Medium | Actual desktop UI is the interactive prototype source | Active |
