# Architecture

## Current architecture

~~~text
Codex / Claude / Cursor / Terminal
                |
        CLI / MCP / future UI
                |
        loopback control API
                |
            agentdockd
                |
      +---------+----------+
      |                    |
reconciliation         HTTP proxy
      |                    |
discovery           hostname resolver
      |                    |
      +------ SQLite ------+
             registry
      projects/services/routes/events
~~~

## Stable identity and routing

Project ID derives from project root.

Project-backed service ID derives from:

- project ID
- protocol
- framework
- executable/process name

Port is excluded.

Each project receives a canonical hostname such as:

~~~text
storefront.localhost
~~~

If two projects have the same name, one keeps the short hostname and the other receives a deterministic suffix such as:

~~~text
app.localhost
app-a1b2c3.localhost
~~~

The proxy resolves only ACTIVE services classified as development.

## Proxy data path

~~~text
browser
  |
Host: storefront.localhost:7777
  |
agentdock-proxy
  |
route registry
  |
active service record
  |
127.0.0.1:<current-port>
~~~

The proxy supports normal HTTP traffic and bidirectional TCP forwarding after the initial HTTP Host header has been resolved, which also keeps WebSocket-style upgrades possible.

## Control API

The control API remains separate from proxied application traffic.

Default:

~~~text
127.0.0.1:7317
~~~

The proxy default:

~~~text
127.0.0.1:7777
~~~

Both refuse non-loopback binds unless the user explicitly passes --allow-non-loopback.

## Persistence

SQLite tables:

- metadata
- projects
- services
- routes
- events

Schema version: 2.

## Next boundary

Phase 4 adds agent/session identity and ownership policy before any destructive process controls are exposed through MCP.
