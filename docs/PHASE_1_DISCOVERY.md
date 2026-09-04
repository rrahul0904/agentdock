# Phase 1 — Reliable Local Discovery

## Goal

Turn the bootstrap scanner into a normalized, cross-platform discovery layer that can safely feed a long-running daemon.

## Data captured

A discovered service can now carry:

- PID
- TCP/UDP protocol
- bind address
- port
- executable/process name
- command line
- working directory
- project identity
- Git-root/worktree hint
- framework/runtime classification
- container runtime hint
- service classification

## Unix strategy

macOS and Linux use `lsof` for listener inventory because it provides a compact, machine-readable mapping from sockets to processes.

A second per-PID enrichment pass retrieves:

- cwd from `lsof -d cwd`
- command line from `ps`

The enrichment pass is cached per PID within one scan so a process with multiple listeners is inspected once.

## Windows strategy

PowerShell combines:

- `Get-NetTCPConnection -State Listen`
- `Get-NetUDPEndpoint`
- `Get-CimInstance Win32_Process`

This provides listener ownership and command metadata without requiring a bundled native dependency.

Windows cwd enrichment is explicitly deferred because `Win32_Process` does not expose cwd reliably. Phase 2 can add agent-launched process ownership, which is more reliable than guessing cwd after the fact.

## Classification

Services are classified as:

- `development`
- `infrastructure`
- `system`
- `unknown`

The CLI displays development and infrastructure by default. `--all` includes system and unknown listeners.

This is a presentation filter only. The discovery model retains all listeners.

## Framework fingerprints

Current command-based fingerprints include:

- Next.js
- Vite
- FastAPI/Uvicorn
- Django
- Rails/Puma
- Spring Boot
- Go
- Node
- Python
- PostgreSQL
- Redis
- Mailhog/Mailpit
- Docker proxy
- Podman

Phase 2/3 should add manifest-aware fingerprints and confidence scores.

## Testing

Parser behavior is fixture-driven for representative `lsof` output. Cross-platform CI compiles/tests the workspace on Ubuntu, macOS, and Windows.

## Safety

Discovery is read-only:

- no process termination
- no port rebinding
- no firewall changes
- no network exposure
- no environment-variable capture
