# Local API

AgentDock exposes a read-only loopback control API from agentdockd.

Default:

~~~text
127.0.0.1:7317
~~~

## Endpoints

GET /healthz

Returns daemon health and version.

GET /v1/status

Returns daemon and registry counts, including route count.

GET /v1/services

Returns development and infrastructure service records.

GET /v1/services?all=1

Also returns system and unknown listeners.

GET /v1/projects

Returns durable project identities and canonical hostnames.

GET /v1/routes

Returns canonical and alias route records.

GET /v1/events?after=<seq>&limit=<n>

Returns lifecycle events after a monotonically increasing cursor.

## Proxy

The HTTP proxy is a separate listener.

Default:

~~~text
127.0.0.1:7777
~~~

Example:

~~~text
http://storefront.localhost:7777
~~~

The proxy returns:

- 404 for an unknown AgentDock hostname
- 503 for a known project with no active development service
- upstream response when the route resolves successfully

## Security

Both control API and proxy refuse non-loopback binds unless --allow-non-loopback is explicitly supplied.

The Phase 3 API remains read-only.
