# Risk Register

| ID | Risk | Level | Mitigation / evidence | State |
|---|---|---:|---|---|
| RSK-001 | Rust toolchain missing at intake | High | Official Rustup installed; `cargo test` and Tauri build verified | Resolved |
| RSK-002 | WebView can expose plaintext if IPC returns it | Critical | Native-only Reveal/Copy; metadata-only command contract | Mitigated; native manual check pending |
| RSK-003 | V1 scope spans desktop, crypto and filesystem | High | Dependency-ordered work items and vertical slice gate | Active |
| RSK-004 | `.env` accidentally tracked by Git | Critical | Fail-closed preflight and integration test | Mitigated |
| RSK-005 | Real provider validation needs private credentials | Medium | Synthetic readiness first; user-local only when required | Deferred |
| RSK-006 | UI prototype drifts from implementation | Medium | Actual desktop UI is the interactive prototype source | Active |
| RSK-007 | Native UI manual acceptance unavailable in current session | Medium | Process launch verified; retain manual acceptance gate for install/unlock/reveal/export | Waiting user |
| RSK-008 | Cross-platform Rust dependency warnings | Medium | `cargo audit` exit 0; warnings recorded and tied to non-Windows Tauri transitive graph | Tracked |
