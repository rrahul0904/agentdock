# Implementation Plan

## Phase 0 - Foundation

Completed: product thesis, repository architecture, Rust workspace, core service model, CLI, MCP bootstrap, CI, security model, and roadmap.

## Phase 1 - Reliable local discovery

Implemented:

- macOS/Linux listener discovery
- Windows listener/PID discovery
- command-line enrichment
- Unix cwd capture
- project/worktree resolution
- framework/runtime classification
- optional UDP discovery
- fixture-driven parser tests

Incremental discovery hardening:

- Windows cwd enrichment
- real container ID/name correlation
- manifest-aware framework fingerprints
- large-inventory benchmarks

## Phase 2 - Durable daemon and registry

Implemented:

- agentdockd
- bundled SQLite
- durable project/service IDs
- periodic reconciliation
- active/stale/orphaned lifecycle
- resume detection
- event cursor
- loopback control API
- CLI daemon client

## Phase 3 - Stable localhost routing

Implemented:

- HTTP reverse proxy
- canonical project .localhost routes
- deterministic collision handling
- persistent route registry
- active-development-service selection
- transparent port changes
- route API/CLI inspection
- loopback-only default binding

Refinements:

- explicit service roles in monorepos
- active HTTP health probes
- alias write API
- installer-managed port 80
- optional local TLS

## Phase 4 - Agent attribution and MCP

Implemented:

- process-ancestry attribution for Codex
- process-ancestry attribution for Claude Code
- process-ancestry attribution for Cursor
- process-ancestry attribution for Gemini CLI
- attribution in daemon reconciliation
- attribution in one-shot CLI discovery
- bounded HTTP request parsing for daemon writes
- owner-scoped port reservations
- owner-scoped release protection
- MCP SDK v2 migration
- daemon-backed agentdock_status
- daemon-backed list_services
- daemon-backed get_project
- daemon-backed list_routes
- daemon-backed reserve_port
- daemon-backed release_port
- daemon-backed get_preview_url
- orphan inventory via cleanup_orphans dry-run
- MCP TypeScript CI build

Intentionally not implemented yet:

- destructive orphan cleanup
- arbitrary shell/process execution
- log retrieval
- persistent agent-session ownership
- Git worktree lifecycle ownership

Those require stronger ownership evidence first.

## Phase 5 - Durable agent/worktree lifecycle

Next:

- AgentSession registry
- process ownership graph
- Git worktree discovery
- branch/revision tracking
- process start/end attribution
- bounded log capture
- health checks
- persistent port reservation ownership
- safe cleanup eligibility rules

## Later

- browser verification
- LAN/mDNS
- secure public previews
- Tauri desktop UX
- team policies, audit, RBAC/SSO
