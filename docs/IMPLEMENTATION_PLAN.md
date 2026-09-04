# Implementation Plan

## Phase 0 — Foundation (current)

Deliverables:

- [x] product thesis
- [x] repository architecture
- [x] Rust workspace
- [x] normalized service model
- [x] process/port discovery draft
- [x] project resolver draft
- [x] port reservation draft
- [x] CLI
- [x] MCP bootstrap
- [x] CI
- [x] security model
- [x] roadmap

Exit criterion: repository builds and tests in CI.

## Phase 1 — Reliable local discovery

Implement:

- robust macOS process discovery
- robust Linux discovery
- Windows process/PID correlation
- executable + argv capture
- cwd capture
- framework detection
- Docker/Podman awareness
- deduplication and reconciliation
- ignored-system-service filters
- fixture-driven parsers

Acceptance:

- identify common Next.js, Vite, FastAPI, Rails, Go, Java, Docker and database listeners
- map PID -> cwd with >95% reliability on supported local fixtures
- no destructive actions

## Phase 2 — Durable daemon and registry

Implement:

- `agentdockd`
- SQLite schema/migrations
- service reconciliation loop
- project identity persistence
- local API
- event subscriptions
- stale/orphan state

Acceptance:

- stable project identity survives process restarts and port changes
- daemon restarts without losing identities
- CLI becomes a client of daemon API

## Phase 3 — Stable localhost routing

Implement:

- reverse proxy
- hostname registry
- collision handling
- aliases
- health-aware routing
- fallback port mode
- optional local TLS later

Acceptance:

- `project.localhost` follows project across port changes
- proxy never exposes a service externally by default

## Phase 4 — Agent attribution + MCP

Implement tools:

- list_services
- get_project
- reserve_port
- release_port
- get_logs
- get_preview_url
- cleanup_orphans

Implement agent/session detection:

- Codex
- Claude Code
- Cursor
- Gemini CLI
- terminal/manual

Acceptance:

- all destructive MCP operations require ownership/policy checks
- parallel port reservations are race-safe

## Phase 5 — Worktrees and lifecycle

- Git worktree registry
- branch/revision tracking
- process ownership
- session end cleanup
- protected-process rules
- CPU/memory telemetry

## Phase 6 — LAN and public previews

- mDNS adapter
- QR preview
- explicit per-project sharing
- Cloudflare Tunnel adapter
- expiring share tokens
- revocation

## Phase 7 — Browser verification

- headless browser adapter
- screenshots
- console/network errors
- route checks
- verification artifact model
- MCP verification tool

## Phase 8 — Desktop UX

Tauri + React:

- service inventory
- project detail
- agent sessions
- worktrees
- logs
- preview controls
- orphan cleanup
- verification timeline

## Phase 9 — Team / commercial layer

Only after strong individual-developer adoption:

- accounts
- team policy
- shared presets
- audit events
- SSO/RBAC
- optional enterprise self-hosting
