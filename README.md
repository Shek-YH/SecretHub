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

## Security

Read [SECURITY.md](SECURITY.md) before using real credentials. Never paste a real API key into chat, tests, fixtures or commits.
