# Open Source Reuse Strategy

The PRD identifies Townrain/API-Key-Manager, rapg, Infisical, Infisical MCP Server, dotenvx, Tene and envkeep as references. V1 initially uses no copied third-party source code. We will prefer small, independently licensed dependencies and record any actual reuse in `THIRD_PARTY_NOTICES.md` before shipping.

The EchoBird `Model Nexus` source was inspected as a behavioral reference for this Work Item: a value-free provider directory supplies website, base URL, default model and optional model ID choices, and the add form consumes that entry. SecretHub copied no EchoBird source, assets, branding or UI layout; its local provider catalog is an independent implementation.

## Planned reference boundaries

- Provider registry and format detection: conceptual reference only until adapters are implemented.
- Runtime injection and agent isolation: deferred to V3; no code copied in V1.
- dotenv parsing/writing: use a mature MIT/Apache-compatible crate only after license and behavior verification.
- Infisical `ee/` and PX Secrets AGPL code: explicitly excluded.

## Compliance gate

Before adding a meaningful dependency, record name, version, license, commercial-use status, redistribution obligations and whether source was copied or only behaviorally referenced.
