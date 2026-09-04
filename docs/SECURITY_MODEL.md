# Security Model

## Default posture

AgentDock is local-first and private-by-default.

Discovery does not imply exposure. A discovered service remains bound exactly as its owning process configured it unless the user explicitly enables an AgentDock route or share.

## Trust boundaries

1. operating system
2. AgentDock daemon
3. local clients (CLI/Desktop/MCP)
4. LAN
5. public internet
6. optional cloud control plane

Each boundary must require an explicit capability.

## Destructive operations

Process termination, orphan cleanup, port release, and worktree cleanup must enforce:

- caller identity when available
- resource ownership
- protected-process denylist
- explicit policy
- audit event
- bounded target selection

Never implement broad commands equivalent to `killall node`.

## MCP

MCP is a powerful local control surface. Rules:

- read-only tools first
- daemon owns authorization logic
- Node MCP bridge does not execute arbitrary shell commands
- tool arguments are structured
- no raw command execution tool
- cleanup tools require ownership evidence
- configurable confirmation/policy for destructive actions

## Public previews

Future public sharing must include:

- cryptographically strong random tokens
- server-side or local hashed token storage
- explicit expiration
- revocation
- route-level allowlist
- no automatic database/admin-port exposure
- rate limiting where applicable
- optional password / identity gate

## Secrets

Do not persist application environment secrets or capture process environments by default.

Logs should be treated as potentially sensitive and redacted where practical.

## Telemetry

Core operation must not require telemetry.

If product analytics are added:

- opt-out
- no source code
- no environment variables
- no captured request bodies
- document every collected field
