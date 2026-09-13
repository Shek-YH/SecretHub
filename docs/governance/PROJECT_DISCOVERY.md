# Project Discovery

## Current state

- Project root: `F:\CCPJ\SecretHub`
- Initial state: PRD only; no Git repository, source code, tests, package metadata or UI prototype.
- Host tools at intake: Node.js 24.11.0, npm 11.6.1, Git 2.53.0; Rust/Cargo unavailable at intake.
- Target: Windows 11 first, macOS later.

## Execution mode

`constrained-single-agent`; no child-agent execution evidence is available in this environment. Each work item has separate implementation and verification evidence.

## Complexity

`COMPLEX`: desktop UI + native core + cryptography + filesystem export + security acceptance.

## Real resources

No login, private credential, real provider key or external account is needed for synthetic development. Real provider validation and manual Windows installation acceptance remain later readiness gates.
