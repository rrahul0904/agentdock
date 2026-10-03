# AgentDock

AgentDock is an AI local development control plane for projects, ports, processes, stable local URLs, worktrees, and coding-agent sessions.

## Current status

Phase 4 agent attribution + daemon-backed MCP baseline is implemented.

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
- Rust + MCP CI configuration
- Codex-style Supervisor Console backed by `supervisor-snapshot/v1`
- confirmed local Forge pause/resume and priority request authoring
- durable Forge control-receipt visibility
- read-only registered-agent-session and policy-delivery receipt visibility

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
cargo run -p agentdock-cli -- daemon events
~~~

Run one-shot discovery:

~~~bash
cargo run -p agentdock-cli -- scan --all
~~~

Run the Supervisor Console from a versioned Forge snapshot:

~~~bash
cargo run -p agentdock-cli -- supervisor --snapshot docs/examples/supervisor.snapshot.example.json
cargo run -p agentdock-cli -- console --snapshot /path/to/autonomous-forge/.ai/supervisor/snapshot.json --forge-root /path/to/autonomous-forge
cargo run -p agentdock-cli -- watch --snapshot /path/to/autonomous-forge/.ai/supervisor/snapshot.json
~~~

The console exposes status, projects, workers, tasks, risks, decisions, snapshot refresh, durable Forge control receipts, and explicit confirmed pause/resume and project/task priority controls. Forge remains authoritative: a queued request is not described as applied until a durable Forge receipt proves it. See `docs/SUPERVISOR_CONSOLE.md`.

Inspect the registered local agent sessions and durable AUTO-049 policy-delivery outcomes without reading policy request payloads:

~~~bash
cargo run -p agentdock-cli --bin agentdock-policy -- sessions --root /path/to/autonomous-forge
cargo run -p agentdock-cli --bin agentdock-policy -- receipts --root /path/to/autonomous-forge --limit 20
cargo run -p agentdock-cli --bin agentdock-policy -- receipts --root /path/to/autonomous-forge --limit 20 --json
~~~

`agentdock-policy` is read-only. It validates `agent-session-registration/v1` and `agent-policy-delivery-receipt/v1`, refuses unsafe/symlinked paths, and never opens session inbox request payloads. See `docs/AGENT_POLICY_SESSIONS.md`.

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

Install package dependencies:

~~~bash
pnpm --dir packages/mcp-server install --no-frozen-lockfile
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
- reserve_port
- release_port
- get_preview_url
- cleanup_orphans (dry-run only for destructive cleanup)

Every MCP process receives its own owner token unless AGENTDOCK_SESSION_ID is explicitly supplied. A session cannot release another session's port reservation.

## Safety boundary

AgentDock does not expose arbitrary shell execution or generic process-kill tools.

Supervisor controls remain typed, bounded local requests. Agent-policy session visibility is read-only. Process-group remediation and policy delivery are owned by Autonomous Forge and require their respective Forge confirmation/evidence contracts.

## Persistence

Default database:

~~~text
~/.agentdock/agentdock.db
~~~

## Repository layout

~~~text
crates/
  agentdock-core/
  agentdock-supervisor/
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

The canonical Local Astra path is to keep widening evidence-backed supervision and agent coordination without bypassing explicit ownership, confirmation, receipt, and verification boundaries.

## License

MIT
