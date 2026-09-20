# AgentDock

AgentDock is an AI local development control plane for projects, ports, processes, stable local URLs, worktrees, and coding-agent sessions.

## Current status

Phase 4 agent attribution + daemon-backed MCP baseline is implemented. The AgentPort donor work now adds a fail-closed remote capability contract, durable coding-agent session inventory/logs, and loopback-only device pairing with explicit local approval and revocation. Remote transport and remote agent input remain disabled.

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

Remote transport and remote agent input remain disabled. Every remote capability requires pairing, and every mutating remote capability also requires explicit approval.

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

The next AgentPort donor wave is an authenticated bidirectional transport bound to an active, non-revoked paired device. Remote agent input remains blocked until transport authentication, reconnect/replay protection, and parameter-bound approvals are implemented and tested. In parallel, AgentDock still needs deeper Git worktree/branch topology, richer log capture, health checks, and only then bounded process cleanup.

## License

MIT
