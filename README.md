# SecretHub

Local-first developer secret and project environment manager for Windows.

## Status

The project is being built from the PRD in dependency order. The first release target is a secure vertical slice: create a master password, save encrypted secret metadata/payload, unlock after restart, select secrets, preview and export a `.env` file.

## Development

```powershell
npm install --prefix apps/desktop
npm run dev
npm test
npm run typecheck
npm run build
cargo test --workspace
npx tauri build --debug
```

The browser E2E uses the local Vite preview and the host-installed Chrome:

```powershell
python C:\Users\Administrator\.codex\skills\webapp-testing\scripts\with_server.py --server "npm --prefix apps/desktop run dev -- --host 127.0.0.1" --port 1420 -- python tests/e2e/golden_path.py
```

The MSI and NSIS bundles are written to `target/debug/bundle`. The actual vault database lives in the per-user Tauri app-data directory, not in the repository.

## CLI and MCP

Build the shared-core CLI with `cargo build -p secrethub-cli`. It supports `status`, metadata-only `catalog`, hidden-input `create`, and `run --secret-id ... -- <program>` runtime injection without writing `.env`; set `SECRETHUB_DB_PATH` to target a specific local vault. Build and smoke-test the metadata-only MCP server with `npm run mcp:smoke`, then run `node apps/mcp/dist/index.js` over stdio.

The MCP server exposes `secrethub_*` catalog/recommendation tools. It opens SQLite read-only, never decrypts payloads, and writes only a value-free pending plan; SecretHub Desktop must show the request and confirm before the existing safe export path writes project files.

For runtime injection, copy `.secrethub.toml.example` to `.secrethub.toml` in a project and list only environment-key names. The file contains no Secret values and can be committed. Run `secrethub-cli run -- <program>` from that project.

The V3.5 proxy policy foundation is in `crates/proxy`: it issues short-lived tokens and validates fixed provider origins without storing token plaintext. HTTP forwarding remains a separate implementation gate.
The forwarding binary is documented in `apps/proxy/README.md`; real-provider smoke remains deliberately unconfigured unless a user enters a test credential locally.

Add Secret is template-first: choose a mainstream domestic/global provider, then choose a curated model or `Custom model ID`. The provider's API-key page, default API-key environment key, model ID and model environment key are prefilled. The API-key environment key is optional; an empty key is retained as a local vault entry but is omitted from `.env` output. Provider templates are local and value-free in `apps/desktop/src/providers.ts`, so they can be reviewed or extended without touching encrypted payload handling.

## Security

Read [SECURITY.md](SECURITY.md) before using real credentials. Never paste a real API key into chat, tests, fixtures or commits.
