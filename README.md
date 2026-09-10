# AgentDock

AgentDock is an AI local development control plane for orchestrating projects, ports, processes, previews, worktrees, and coding agents.

## Current status

Phase 2 durable daemon and registry baseline is implemented.

AgentDock now includes:

- cross-platform service discovery
- project and framework resolution
- SQLite-backed durable registry
- stable project and service IDs
- active -> stale -> orphaned lifecycle tracking
- resume detection
- append-only lifecycle event log
- periodic daemon reconciliation
- read-only loopback HTTP API
- CLI daemon client
- fixture/unit tests
- cross-platform CI configuration

## Run locally

One-shot discovery:

~~~bash
cargo run -p agentdock-cli -- scan
~~~

Start the daemon:

~~~bash
cargo run -p agentdockd
~~~

Default database:

~~~text
~/.agentdock/agentdock.db
~~~

Inspect the daemon from another terminal:

~~~bash
cargo run -p agentdock-cli -- daemon status
cargo run -p agentdock-cli -- daemon services
cargo run -p agentdock-cli -- daemon services --all
cargo run -p agentdock-cli -- daemon projects
cargo run -p agentdock-cli -- daemon events
~~~

Custom daemon settings:

~~~bash
cargo run -p agentdockd -- --bind 127.0.0.1:7317 --interval-ms 2000 --orphan-after-ms 30000 --db /tmp/agentdock.db
~~~

## Local API

- GET /healthz
- GET /v1/status
- GET /v1/services
- GET /v1/services?all=1
- GET /v1/projects
- GET /v1/events?after=0&limit=200

See docs/API.md.

## Durable identity

Project-backed service identity excludes the port. A project service can move from port 3000 to 3007 while retaining the same AgentDock service ID.

Lifecycle:

~~~text
observed -> ACTIVE
missed scan -> STALE
missing beyond threshold -> ORPHANED
reappears -> ACTIVE
~~~

## Repository layout

~~~text
crates/
  agentdock-core/
  process-discovery/
  project-resolver/
  framework-detection/
  port-manager/
  agentdock-registry/
  agentdockd/
  agentdock-cli/

packages/
  mcp-server/

docs/
  PRODUCT_VISION.md
  PRODUCT_REVERSE_ENGINEERING.md
  ARCHITECTURE.md
  IMPLEMENTATION_PLAN.md
  PHASE_1_DISCOVERY.md
  PHASE_2_DAEMON.md
  API.md
  SECURITY_MODEL.md
  MCP_DESIGN.md
  ROADMAP.md
  TESTING.md
~~~

## Next

Phase 3 adds the reverse proxy and stable *.localhost routing on top of the durable registry.

## License

MIT
