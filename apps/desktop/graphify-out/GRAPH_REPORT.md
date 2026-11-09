# Graph Report - desktop  (2026-10-15)

## Corpus Check
- 20 files · ~18,884 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 263 nodes · 583 edges · 15 communities (14 shown, 1 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `30d0a8bf`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- App.tsx
- String
- commands.rs
- compilerOptions
- devDependencies
- tauri.conf.json
- package.json
- compilerOptions
- database_path
- tsconfig.json

## God Nodes (most connected - your core abstractions)
1. `AppState` - 38 edges
2. `vault_from_state()` - 37 edges
3. `touch_activity()` - 25 edges
4. `compilerOptions` - 18 edges
5. `isTauriRuntime()` - 13 edges
6. `selected_entries()` - 12 edges
7. `compilerOptions` - 12 edges
8. `export_env()` - 11 edges
9. `lock()` - 10 edges
10. `secret_validate_many()` - 10 edges

## Surprising Connections (you probably didn't know these)
- `secret_import()` --references--> `AppState`  [EXTRACTED]
  src-tauri/src/commands.rs → src-tauri/src/lib.rs
- `mcp_confirm_plan()` --references--> `AppState`  [EXTRACTED]
  src-tauri/src/commands.rs → src-tauri/src/lib.rs
- `SettingsPanel()` --calls--> `isTauriRuntime()`  [EXTRACTED]
  src/App.tsx → src/lib/backend.ts
- `ProfilePrompt()` --calls--> `isTauriRuntime()`  [EXTRACTED]
  src/App.tsx → src/lib/backend.ts
- `ProfilePanel()` --calls--> `isTauriRuntime()`  [EXTRACTED]
  src/App.tsx → src/lib/backend.ts

## Import Cycles
- None detected.

## Communities (15 total, 1 thin omitted)

### Community 0 - "App.tsx"
Cohesion: 0.06
Nodes (41): App(), AuditPanel(), CredentialPreset, CredentialType, demoSecrets, demoUsageCounts, ExportPanel(), ImportPanel() (+33 more)

### Community 1 - "String"
Cohesion: 0.16
Nodes (51): AuditEvent, AutoLockPolicy, Mutex, MutexGuard, Path, ProjectRecord, Settings, audit_list() (+43 more)

### Community 2 - "commands.rs"
Cohesion: 0.13
Nodes (32): EnvEntry, choose_project_directory(), default_category(), default_scope(), default_value_type(), entry_with_optional_comment(), ImportRequest, is_env_key() (+24 more)

### Community 3 - "compilerOptions"
Cohesion: 0.08
Nodes (24): DOM, DOM.Iterable, ES2022, src, vitest/globals, compilerOptions, allowJs, allowSyntheticDefaultImports (+16 more)

### Community 4 - "devDependencies"
Cohesion: 0.09
Nodes (23): jsdom, devDependencies, jsdom, @tauri-apps/cli, @testing-library/jest-dom, @testing-library/react, @testing-library/user-event, @types/react (+15 more)

### Community 5 - "tauri.conf.json"
Cohesion: 0.09
Nodes (21): icons/128x128@2x.png, icons/128x128.png, icons/32x32.png, icons/icon.ico, app, security, windows, build (+13 more)

### Community 6 - "package.json"
Cohesion: 0.11
Nodes (17): dependencies, react, react-dom, @tauri-apps/api, name, private, scripts, build (+9 more)

### Community 7 - "compilerOptions"
Cohesion: 0.12
Nodes (16): ES2023, vite.config.ts, vitest.config.ts, compilerOptions, allowImportingTsExtensions, lib, module, moduleDetection (+8 more)

### Community 8 - "database_path"
Cohesion: 0.32
Nodes (7): database_path(), now_seconds(), AppHandle, PathBuf, Result, String, run()

## Knowledge Gaps
- **88 isolated node(s):** `name`, `private`, `version`, `type`, `dev` (+83 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `String` to `database_path`, `commands.rs`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **Why does `devDependencies` connect `devDependencies` to `package.json`?**
  _High betweenness centrality (0.018) - this node is a cross-community bridge._
- **What connects `name`, `private`, `version` to the rest of the system?**
  _88 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `App.tsx` be split into smaller, more focused modules?**
  _Cohesion score 0.06397306397306397 - nodes in this community are weakly interconnected._
- **Should `commands.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.13012477718360071 - nodes in this community are weakly interconnected._
- **Should `compilerOptions` be split into smaller, more focused modules?**
  _Cohesion score 0.08 - nodes in this community are weakly interconnected._
- **Should `devDependencies` be split into smaller, more focused modules?**
  _Cohesion score 0.08695652173913043 - nodes in this community are weakly interconnected._