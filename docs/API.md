# Local API

AgentDock Phase 2 exposes a read-only loopback HTTP API from agentdockd.

Default address:

~~~text
127.0.0.1:7317
~~~

## Endpoints

GET /healthz

Returns daemon health and version.

GET /v1/status

Returns daemon and registry counts.

GET /v1/services

Returns development and infrastructure service records.

GET /v1/services?all=1

Also returns system and unknown listeners.

GET /v1/projects

Returns durable project identities.

GET /v1/events?after=<seq>&limit=<n>

Returns lifecycle events after a monotonically increasing cursor.

Current event kinds:

- service.discovered
- service.stale
- service.orphaned
- service.resumed

## Security

The default API binds only to loopback. Non-loopback authenticated mode is intentionally deferred.
