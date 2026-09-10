# Implementation Plan

## Phase 0 - Foundation

Completed: product thesis, repository architecture, Rust workspace, service model, project resolver, port reservation, CLI, MCP bootstrap, CI, security model, and roadmap.

## Phase 1 - Reliable local discovery

Implemented:

- macOS/Linux listener discovery
- Windows listener/PID discovery
- command-line enrichment
- working-directory capture on Unix
- project/worktree resolution
- framework/runtime classification
- service classification
- optional UDP discovery
- fixture-driven parser tests

Incremental hardening remains:

- Windows cwd enrichment
- real container ID/name correlation
- manifest-aware framework fingerprints
- large inventory benchmarks

## Phase 2 - Durable daemon and registry

Implemented:

- agentdockd
- SQLite schema and migrations
- bundled SQLite
- periodic reconciliation
- durable project IDs
- durable service IDs
- active/stale/orphaned lifecycle
- resume detection
- append-only event log
- cursor-based event polling
- read-only loopback HTTP API
- CLI daemon client
- restart-safe persistence

## Phase 3 - Stable localhost routing

Implemented:

- local HTTP reverse proxy
- canonical project .localhost route registry
- deterministic hostname collision handling
- route persistence in SQLite
- hostname -> active development service resolution
- port-change transparency
- 404 for unknown routes
- 503 when a project has no active HTTP service
- loopback-only proxy binding by default
- configurable fallback proxy port
- route inspection via API and CLI
- proxy parser/unit tests
- registry route collision/port-change tests

Known refinements:

- explicit service roles for monorepos with multiple same-framework listeners
- request/response health probes beyond listener presence
- user-facing alias management API
- privileged/service-installed port 80 mode
- optional local TLS

## Phase 4 - Agent attribution and MCP

Next:

- move MCP tools onto daemon API
- list_services
- get_project
- reserve_port
- release_port
- get_logs
- get_preview_url
- cleanup_orphans
- Codex attribution
- Claude Code attribution
- Cursor attribution
- Gemini attribution
- caller/session ownership policy

## Later phases

- worktree topology and lifecycle
- browser verification
- LAN/public preview adapters
- Tauri desktop UX
- team policy / RBAC / enterprise self-hosting
