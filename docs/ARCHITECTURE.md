# Architecture

## Phase 2 architecture

~~~text
CLI / MCP / future desktop
          |
    loopback HTTP API
          |
      agentdockd
          |
  +-------+--------+
  |                |
reconciliation   SQLite registry
  |                |
discovery       projects/services/events
~~~

## Registry model

Project:
- stable ID
- root
- Git root/worktree hint
- first seen
- last seen

Service:
- stable ID
- project ID
- latest normalized snapshot
- lifecycle state
- first seen
- last seen
- missing since

Event:
- monotonic sequence
- lifecycle event
- entity ID
- JSON payload
- timestamp

## Stable identity

Project ID derives from project root.

For project-backed services, service ID derives from:

- project ID
- protocol
- framework
- executable/process name

Port is intentionally excluded so restarts can move ports without changing AgentDock identity.

Unknown/unowned services retain bind address and port in the fallback identity to avoid accidental collapsing.

## Reconciliation lifecycle

~~~text
ACTIVE --missed--> STALE --threshold--> ORPHANED
  ^                                  |
  +------------ observed ------------+
~~~

## Local API

Phase 2 uses a small HTTP/1.1 loopback server built on std::net::TcpListener.

The API is read-only. This avoids introducing destructive remote operations before caller identity and resource ownership are implemented.

## Event subscriptions

Clients poll:

~~~text
GET /v1/events?after=<last_seq>
~~~

SQLite supplies a monotonic cursor. This is sufficient for the CLI, MCP bridge, and first desktop client.

## Phase 3

The next layer is a local reverse proxy that resolves stable hostnames from the durable service registry.
