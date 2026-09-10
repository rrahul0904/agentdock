# Architecture

## Current architecture

~~~text
Codex / Claude Code / Cursor / Gemini / Terminal
                       |
                 process ancestry
                       |
                Agent attribution
                       |
          +------------+------------+
          |                         |
        MCP v2                     CLI
          |                         |
          +------------+------------+
                       |
               loopback control API
                       |
                   agentdockd
                       |
      +----------------+-----------------+
      |                |                 |
reconciliation     port manager       HTTP proxy
      |                |                 |
discovery       owner reservations   route resolver
      |                                  |
      +--------------- SQLite -----------+
              projects/services/routes/events
~~~

## Discovery and agent attribution

The discovery layer maps listeners to PID, command, cwd, project, framework, and lifecycle data.

Agent attribution then walks a bounded parent-process chain to identify known coding-agent ancestors:

- Codex
- Claude Code
- Cursor
- Gemini CLI

The current session identifier is heuristic: agent kind plus detected ancestor PID.

Persistent session identity belongs to the next lifecycle phase.

## Durable identity

Project ID derives from project root.

Project-backed service ID derives from:

- project ID
- protocol
- framework
- executable/process name

Port is intentionally excluded so restarts can change ports without changing AgentDock identity.

## Stable localhost routing

Each project receives a canonical route such as:

~~~text
storefront.localhost
~~~

Same-name projects receive a deterministic suffix.

The proxy selects only ACTIVE development services.

Default proxy:

~~~text
127.0.0.1:7777
~~~

## Daemon control API

Default:

~~~text
127.0.0.1:7317
~~~

The daemon has two classes of operations.

Read operations:

- status
- services
- projects
- routes
- events
- preview lookup

Narrow write operations:

- reserve a port for an explicit owner
- release a port only for the same owner

There is no arbitrary shell or generic process-termination API.

## MCP boundary

The TypeScript MCP bridge does not manage OS resources directly.

~~~text
MCP host
   |
session owner token
   |
MCP stdio bridge
   |
structured daemon API
   |
Rust ownership/policy boundary
~~~

The bridge injects its own reservation owner token so a model cannot impersonate another MCP session when releasing ports.

## Cleanup boundary

Lifecycle state alone is not enough evidence to terminate a process.

Destructive orphan cleanup remains disabled until AgentDock can persist a trustworthy graph:

~~~text
AgentSession
    |
    +-- Worktree / branch / revision
    |
    +-- Process ownership
            |
            +-- Service
            |
            +-- port reservation
~~~

## Persistence

SQLite currently stores:

- metadata
- projects
- services
- routes
- lifecycle events

Port reservations are intentionally in-memory and expire after 60 seconds.

Persistent AgentSession and reservation ownership are Phase-5 work.

## Network security

Control API and proxy bind to loopback by default.

Non-loopback binding requires explicit --allow-non-loopback acknowledgement.

## Next architecture layer

Phase 5 introduces durable AgentSession/worktree/process ownership, bounded logs, service health, and cleanup eligibility rules.
