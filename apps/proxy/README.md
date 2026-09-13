# SecretHub Local Agent Proxy

`secrethub-proxy-http` is the V3.5 local forwarding process. It unlocks the local vault interactively, reads one selected Secret in Rust memory, binds only to the requested local address, and gives the Agent a short-lived proxy token. The Agent never receives the long-lived provider value.

Example:

```powershell
$env:SECRETHUB_DB_PATH = 'C:\path\to\vault.sqlite'
cargo run -p secrethub-proxy-http -- --provider openai --secret-id secret-001 --listen 127.0.0.1:0
```

The process prints only the local proxy URL, short-lived token and expiry. It forwards a small allowlisted set of request headers to fixed HTTPS Provider origins, rejects redirects, limits request/response sizes, and emits no request or Secret logs. It exits when interrupted.
