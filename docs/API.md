# Local API

AgentDock exposes a loopback control API from agentdockd.

Default:

~~~text
127.0.0.1:7317
~~~

## Read endpoints

### GET /healthz

Daemon liveness and version.

### GET /v1/status

Daemon, proxy, and durable registry status.

### GET /v1/services

Development/infrastructure services.

Use all=1 to include system and unknown listeners.

### GET /v1/projects

Durable projects and canonical hostnames.

### GET /v1/routes

Persistent localhost routes.

### GET /v1/events?after=<seq>&limit=<n>

Lifecycle events after a monotonic cursor.

### GET /v1/preview?project_id=<id>

Returns the stable local preview URL for a project.

## Structured write endpoints

### POST /v1/ports/reserve

Body:

~~~json
{
  "owner": "session-token"
}
~~~

Returns a free port reserved for 60 seconds.

### POST /v1/ports/release

Body:

~~~json
{
  "owner": "session-token",
  "port": 4317
}
~~~

Release succeeds only when the owner matches the reservation.

Malformed JSON returns a structured HTTP 400.

## Proxy

Default:

~~~text
127.0.0.1:7777
~~~

Example:

~~~text
http://storefront.localhost:7777
~~~

Unknown route -> 404.

Known route without active development service -> 503.

## Security

Control API and proxy refuse non-loopback binds unless --allow-non-loopback is explicitly supplied.

The API does not expose arbitrary process termination or shell commands.
