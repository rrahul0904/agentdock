# AgentDock

AgentDock is an AI local development control plane for projects, ports, processes, stable local URLs, worktrees, and coding-agent sessions.

## Current status

Phase 4 agent attribution + daemon-backed MCP baseline is implemented. The AgentPort donor work now adds a fail-closed remote capability contract, durable coding-agent session inventory/logs, loopback-only device pairing with explicit local approval and revocation, Ed25519 device-possession proof, and an opt-in outbound authenticated read-only WSS relay client. Remote agent input and remote action execution remain disabled.

AgentDock now includes:

- cross-platform service discovery
- project/framework resolution
- coding-agent ancestry attribution for Codex, Claude Code, Cursor, and Gemini CLI
- SQLite-backed durable registry
- stable project/service IDs
- active -> stale -> orphaned lifecycle tracking
- canonical .localhost hostnames
- deterministic hostname collision handling
- local HTTP reverse proxy
- loopback control API
- owner-scoped port reservations
- MCP TypeScript v2 server backed by the daemon
- durable coding-agent sessions derived from attributed local agents
- bounded lifecycle logs for coding-agent sessions
- fail-closed remote capability discovery with pairing/approval requirements
- short-lived one-time pairing challenges stored only as hashes
- pending device requests with explicit local approve/deny
- durable paired-device revocation and audit events
- one-time durable remote-auth challenges with Ed25519 possession proof
- replay-protected remote transport sessions with fresh reconnect epochs
- parameter-bound, expiring, one-shot remote approval records
- opt-in outbound WSS relay for authenticated session inventory and logs only
- Rust + MCP CI configuration

## Run locally

Start the daemon:

~~~bash
cargo run -p agentdockd
~~~

Inspect it:

~~~bash
cargo run -p agentdock-cli -- daemon status
cargo run -p agentdock-cli -- daemon services --all
cargo run -p agentdock-cli -- daemon projects
cargo run -p agentdock-cli -- daemon routes
cargo run -p agentdock-cli -- daemon sessions
cargo run -p agentdock-cli -- daemon session-logs <session-id>
cargo run -p agentdock-cli -- daemon events
~~~

Manage paired devices locally:

~~~bash
cargo run -p agentdock-cli -- pairing create
cargo run -p agentdock-cli -- pairing requests
cargo run -p agentdock-cli -- pairing approve <request-id>
cargo run -p agentdock-cli -- pairing deny <request-id>
cargo run -p agentdock-cli -- pairing devices
cargo run -p agentdock-cli -- pairing revoke <device-id>
~~~

Run one-shot discovery:

~~~bash
cargo run -p agentdock-cli -- scan --all
~~~

## Optional remote relay

The remote relay client is disabled by default. To enable the repository-certified read-only outbound transport, provide a WSS endpoint and a relay-issued bearer token:

~~~bash
export AGENTDOCK_RELAY_URL="wss://relay.example.com/agentdock"
export AGENTDOCK_RELAY_TOKEN="<relay-issued-token>"
cargo run -p agentdockd
~~~

The URL can instead be passed with `--remote-relay-url`. The bearer token is intentionally environment-only so it does not need to appear in the process command line.

The relay client permits only authenticated session inventory and session-log requests. It requires an active, non-revoked paired device to prove possession of its Ed25519 private key, creates a fresh transport session/epoch after successful proof, rejects replayed sequence numbers, and closes the durable transport session when the socket disconnects or fails.

## Stable localhost routing

Default proxy:

~~~text
127.0.0.1:7777
~~~

A project named storefront is reachable at:

~~~text
http://storefront.localhost:7777
~~~

The same URL follows the project when its underlying server changes ports.

## MCP server

Install workspace dependencies:

~~~bash
pnpm install
~~~

Run the stdio MCP server:

~~~bash
pnpm mcp:dev
~~~

The MCP bridge connects to agentdockd at:

~~~text
127.0.0.1:7317
~~~

Override with AGENTDOCK_ADDR.

Implemented MCP tools:

- agentdock_status
- list_services
- get_project
- list_routes
- list_agent_sessions
- get_agent_session_logs
- reserve_port
- release_port
- get_preview_url
- cleanup_orphans (dry-run only for destructive cleanup)

Every MCP process receives its own owner token unless AGENTDOCK_SESSION_ID is explicitly supplied. A session cannot release another session's port reservation.

## Safety boundary

AgentDock does not expose arbitrary shell execution or generic process-kill tools.

The remote capability contract is discoverable locally. Pairing and device revocation are implemented as loopback-only administrative operations with short-lived one-time secrets, explicit local approve/deny, lockout after repeated invalid secrets, and durable audit state. These administrative operations are intentionally not exposed as MCP tools.

The optional relay is outbound-only, WSS-only, does not follow redirects, does not log its bearer token, and currently accepts only `SessionInventory` and `SessionLogs`. A paired remote device must prove possession of its Ed25519 private key against a short-lived one-time challenge before a durable replay-protected transport session is opened.

Remote `AgentInput` and `ActionApproval` execution remain disabled even though parameter-bound approval records exist in the registry. Every remote capability requires pairing, and every mutating remote capability also requires explicit approval.

cleanup_orphans can inventory orphaned services, but destructive cleanup remains disabled until process/session ownership can be proved reliably.

## Persistence

Default database:

~~~text
~/.agentdock/agentdock.db
~~~

## Repository layout

~~~text
crates/
  agentdock-core/
  process-discovery/
  project-resolver/
  framework-detection/
  agent-attribution/
  port-manager/
  agentdock-registry/
  agentdock-proxy/
  agentdockd/
  agentdock-cli/

packages/
  mcp-server/
~~~

## Next

The next AgentPort donor wave is hosted relay/browser interoperability and end-to-end certification of the already implemented authenticated read-only transport. Only after that external path is verified should remote agent input be wired to the existing parameter-bound one-shot approval records. In parallel, AgentDock still needs deeper Git worktree/branch topology, richer log capture, health checks, and only then bounded process cleanup.

## License

MIT
