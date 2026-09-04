# Architecture

## Design goals

- local-first core
- cross-platform daemon
- explicit security boundaries
- adapter-driven integrations
- versioned APIs
- no cloud dependency for core discovery/orchestration
- durable enough to handle many concurrent agents and services

## Logical architecture

```text
+----------------------+       +----------------------+
| Desktop / CLI / MCP  |       | Coding agents        |
+----------+-----------+       +----------+-----------+
           |                              |
           +--------------+---------------+
                          |
                    Local API
                          |
                +---------v---------+
                | AgentDock daemon  |
                +---------+---------+
                          |
      +-------------------+--------------------+
      |                   |                    |
+-----v------+     +------v------+      +------v-------+
| Discovery |     | Registry    |      | Lifecycle    |
| processes |     | projects    |      | ownership    |
| ports     |     | services    |      | cleanup      |
+-----+------+     +------+------+      +------+-------+
      |                   |                    |
      +-------------------+--------------------+
                          |
                +---------v---------+
                | Local persistence |
                | SQLite (Phase 2)  |
                +-------------------+

Optional adapters:
- *.localhost reverse proxy
- mDNS/LAN sharing
- Cloudflare Tunnel
- browser verification
- container runtimes
- Git providers
```

## Phase 1 discovery pipeline

```text
OS listener inventory
   |
   +-- macOS/Linux: lsof
   |
   +-- Windows: Get-NetTCPConnection / Get-NetUDPEndpoint
   |
PID + bind address + port
   |
process enrichment
   |
command + command line + cwd
   |
project resolver
   |
framework/container classifier
   |
normalized Service model
   |
CLI / future daemon reconciliation
```

Discovery is observational. It never kills or exposes processes.

## Core domain model

### Project
Canonical developer-facing identity for a source tree.

### Worktree
Specific Git worktree/branch checkout associated with a project.

### AgentSession
An identified coding-agent or terminal session.

### Process
Operating-system process observed by AgentDock.

### Service
A reachable listener exposed by a process.

### PortReservation
Short-lived reservation preventing two agents from racing for a port.

### Preview
A local, LAN, or public route to a service.

### VerificationRun
A browser/test-based validation result tied to a service and revision.

## Phase 2 daemon

Introduce a long-running Rust daemon:

- Unix domain socket on macOS/Linux
- named pipe on Windows
- optional loopback HTTP API for UI adapters
- SQLite registry
- periodic discovery reconciliation
- event stream for UI/MCP subscribers

## Local proxy

Later, a reverse proxy listens on a configurable local port and routes by Host header:

```text
storefront.localhost -> service_id=abc -> 127.0.0.1:3001
api.localhost        -> service_id=def -> 127.0.0.1:8012
```

The registry changes when underlying ports change; the human-facing hostname remains stable.

## Cloud architecture

Core AgentDock does not require a cloud backend.

Optional future control-plane services may handle:

- account/team management
- license/subscription state
- public preview metadata
- shared team policies
- remote audit events

Application traffic should not be proxied through our control plane when an existing tunnel provider can handle it safely and cheaply.
