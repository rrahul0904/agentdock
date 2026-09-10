# Phase 3 - Stable Localhost Routing

## Goal

Make a project reachable by stable local identity instead of an ephemeral port.

## Default behavior

agentdockd launches:

- control API on 127.0.0.1:7317
- HTTP proxy on 127.0.0.1:7777

A project named storefront is reachable at:

~~~text
http://storefront.localhost:7777
~~~

## Port 80 mode

Bare project.localhost URLs require the proxy to listen on port 80.

~~~bash
cargo run -p agentdockd -- --proxy-bind 127.0.0.1:80
~~~

Whether this succeeds depends on OS permissions and how AgentDock is installed.

## Canonical hostname allocation

The registry assigns a canonical hostname when a project is first reconciled.

Normal:

~~~text
dashboard.localhost
~~~

Collision:

~~~text
app.localhost
app-4ab921.localhost
~~~

The suffix derives deterministically from the stable project ID.

## Resolution rules

The proxy forwards only when:

- hostname exists in the route registry
- project has an ACTIVE service
- selected service classification is development

Infrastructure, system, unknown, stale, and orphaned services are excluded.

## Port changes

Because the route resolves against the latest registry snapshot, a service can restart on a new port without changing the browser URL.

## Error behavior

- unknown hostname -> HTTP 404
- known project with no active service -> HTTP 503
- upstream connection error -> connection/proxy error

## Safety

The proxy binds to loopback by default.

A non-loopback bind is refused unless the user explicitly passes:

~~~text
--allow-non-loopback
~~~

This prevents accidental LAN exposure.

## Deferred improvements

- alias management through control API
- explicit service-role selection
- active HTTP health probes
- port 80 installer/service integration
- optional local TLS
