# AgentDock

AgentDock is an AI local development control plane for orchestrating projects, ports, processes, stable local URLs, worktrees, and coding agents.

## Current status

Phase 3 stable localhost routing baseline is implemented.

AgentDock now includes:

- cross-platform service discovery
- project and framework resolution
- SQLite-backed durable registry
- stable project and service IDs
- active -> stale -> orphaned lifecycle tracking
- periodic daemon reconciliation
- canonical .localhost hostnames
- deterministic hostname collision handling
- local HTTP reverse proxy
- read-only loopback control API
- CLI daemon client
- route/event inspection
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

The control API listens on:

~~~text
127.0.0.1:7317
~~~

The local proxy listens on:

~~~text
127.0.0.1:7777
~~~

If a project is named storefront, browse:

~~~text
http://storefront.localhost:7777
~~~

If you explicitly bind the proxy to port 80 and your OS allows it:

~~~bash
cargo run -p agentdockd -- --proxy-bind 127.0.0.1:80
~~~

then the bare URL becomes:

~~~text
http://storefront.localhost
~~~

## Durable routing

A project service can move from port 3000 to 3007 while keeping the same AgentDock identity and hostname.

~~~text
storefront.localhost:7777
        |
        v
AgentDock canonical route
        |
        v
latest ACTIVE development service
        |
        +-- 3000
        |
        +-- restart
        |
        +-- 3007
~~~

Stale, orphaned, infrastructure, system, and unknown listeners are not selected as HTTP proxy targets.

## CLI

~~~bash
cargo run -p agentdock-cli -- daemon status
cargo run -p agentdock-cli -- daemon services
cargo run -p agentdock-cli -- daemon services --all
cargo run -p agentdock-cli -- daemon projects
cargo run -p agentdock-cli -- daemon routes
cargo run -p agentdock-cli -- daemon events
~~~

## Local API

- GET /healthz
- GET /v1/status
- GET /v1/services
- GET /v1/services?all=1
- GET /v1/projects
- GET /v1/routes
- GET /v1/events?after=0&limit=200

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
  port-manager/
  agentdock-registry/
  agentdock-proxy/
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
  PHASE_3_PROXY.md
  API.md
  SECURITY_MODEL.md
  MCP_DESIGN.md
  ROADMAP.md
~~~

## Next

Phase 4 moves the MCP bridge onto the daemon API and adds agent/session attribution, ownership-aware port reservations, and safe cleanup.

## License

MIT
