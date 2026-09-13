# SecretHub MCP Server

Local stdio MCP server for the SecretHub metadata catalog.

## Safety boundary

The server opens the local SQLite database read-only and exposes only metadata. It never reads or decrypts `secret_payloads`, never returns a Secret value, never writes `.env`, and never writes to stdout except valid MCP messages. `secrethub_prepare_project_env` returns a confirmation-required plan for SecretHub Desktop to approve.

Set `SECRETHUB_DB_PATH` when the database is not in the default Windows Tauri app-data location:

```powershell
$env:SECRETHUB_DB_PATH = 'C:\path\to\vault.sqlite'
npm run build
node dist/index.js
```
