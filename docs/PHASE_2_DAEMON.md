# Phase 2 - Durable Daemon and Registry

## Goal

Convert AgentDock from a one-shot scanner into a durable local control plane.

## Implemented

agentdockd now:

1. scans local listeners
2. enriches them with project/framework metadata
3. reconciles observations into SQLite
4. tracks lifecycle transitions
5. exposes a read-only local API
6. repeats on a configurable interval

SQLite tables:

- metadata
- projects
- services
- events

## Stable identity

Project IDs derive from project root.

For project-backed services, the identity uses project ID + protocol + framework + process name. Port is excluded so a service restart can move ports without losing identity.

This heuristic will later be extended with explicit service roles for monorepos.

## Lifecycle

- active: observed now
- stale: missed at least once
- orphaned: missing beyond threshold
- resumed: stale/orphaned service observed again

## Persistence

Default:

~~~text
~/.agentdock/agentdock.db
~~~

Override with --db or AGENTDOCK_HOME.

## Safety

Phase 2 remains read-only at the API boundary:

- no process termination
- no port rebinding
- no LAN/public exposure
- no environment capture

Destructive operations wait for resource ownership and caller policy.
