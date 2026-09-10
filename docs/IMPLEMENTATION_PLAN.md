# Implementation Plan

## Phase 0 - Foundation

Completed: product thesis, repository architecture, Rust workspace, service model, project resolver, port reservation, CLI, MCP bootstrap, CI, security model, and roadmap.

## Phase 1 - Reliable local discovery

Implemented:

- macOS/Linux listener discovery with lsof
- Windows listener/PID discovery with PowerShell
- command-line enrichment on Unix
- cwd capture on Unix
- bind-address capture
- project marker fallback
- Git worktree marker recognition
- framework/runtime detection
- Docker/Podman hints
- service classification
- optional UDP discovery
- fixture-driven parser tests
- Windows parser fixture
- cross-platform CI matrix

Incremental hardening still available:

- Windows cwd enrichment
- real container ID/name correlation
- manifest-aware framework fingerprints
- large inventory benchmarks

## Phase 2 - Durable daemon and registry

Implemented:

- agentdockd
- SQLite schema/migration baseline
- bundled SQLite
- periodic service reconciliation
- durable project IDs
- durable service IDs
- stable identity across project port changes
- active/stale/orphaned lifecycle
- resume detection
- append-only lifecycle event log
- cursor-based event polling
- read-only local HTTP API
- CLI daemon client
- restart-safe persistence model
- in-memory reconciliation tests

Known refinements:

- explicit service roles for monorepos with multiple same-framework listeners
- authenticated non-loopback API mode
- install/start-at-login service packaging

## Phase 3 - Stable localhost routing

Next:

- reverse proxy
- hostname registry
- collision handling
- aliases
- health-aware routing
- fallback port mode
- proxy tests

Acceptance:

- project.localhost follows a project across port changes
- proxy never exposes a service externally by default

## Phase 4 - Agent attribution and MCP

- list_services
- get_project
- reserve_port
- release_port
- get_logs
- get_preview_url
- cleanup_orphans
- Codex / Claude Code / Cursor / Gemini attribution
- caller ownership policy

## Later phases

- worktree topology and lifecycle
- browser verification
- LAN/public preview adapters
- Tauri desktop UX
- team policy / RBAC / enterprise self-hosting
