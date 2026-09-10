# Security Model

## Default posture

AgentDock is local-first and private-by-default.

Discovery does not imply exposure.

The control API and proxy bind to loopback by default and reject non-loopback addresses unless the user explicitly acknowledges the risk.

## Trust boundaries

1. operating system
2. agentdockd
3. local CLI / MCP / desktop clients
4. LAN
5. public internet
6. optional future cloud control plane

## Discovery privacy

AgentDock may inspect:

- process name
- command line
- cwd
- process ancestry
- bind address
- port

It does not intentionally collect process environments, source code, request bodies, or application secrets.

Command lines can contain secrets; future durable process/log storage must redact before persistence.

## MCP ownership

Each MCP bridge instance creates a session owner token unless AGENTDOCK_SESSION_ID is supplied by the host.

Port reservation ownership is injected by the bridge rather than model-controlled.

A release request for a reservation owned by another session fails.

## Destructive operations

There is currently no generic process-kill API.

cleanup_orphans is dry-run only.

Before destructive cleanup is enabled AgentDock must have evidence connecting:

~~~text
AgentSession -> Process -> Service -> Project/Worktree
~~~

and apply:

- ownership checks
- protected-process denylist
- bounded targets
- audit events
- explicit policy

Never implement broad behavior equivalent to killall node.

## MCP surface

MCP exposes structured domain tools only.

It does not expose:

- raw shell
- unrestricted filesystem reads
- arbitrary command execution
- environment dumps
- generic PID termination

## Public previews

Future public sharing must add explicit expiration, revocation, cryptographically strong access tokens, route allowlists, and rate controls.
