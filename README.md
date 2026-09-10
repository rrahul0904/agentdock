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
- reserve_port
- release_port
- get_preview_url
- cleanup_orphans (dry-run only for destructive cleanup)

Every MCP process receives its own owner token unless AGENTDOCK_SESSION_ID is explicitly supplied. A session cannot release another session's port reservation.

## Safety boundary

AgentDock does not expose arbitrary shell execution or generic process-kill tools.

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

The next implementation wave is deeper ownership/lifecycle tracking: durable agent sessions, Git worktree/branch topology, log capture, health checks, and only then bounded process cleanup.

## License

MIT
