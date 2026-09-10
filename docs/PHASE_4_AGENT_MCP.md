# Phase 4 - Agent Attribution and MCP

## Goal

Make AgentDock useful directly to coding agents without exposing unsafe generic machine control.

## Agent attribution

For each discovered service with a PID, AgentDock walks a bounded parent-process chain.

Current fingerprints:

- Codex
- Claude Code
- Cursor
- Gemini CLI

The detected agent identity is stored in the normalized service snapshot.

Current session IDs are heuristic and based on agent kind + detected ancestor PID. Durable session identity is a Phase-5 concern.

## MCP v2

The TypeScript bridge uses stdio and talks only to the loopback daemon API.

Implemented tools:

- agentdock_status
- list_services
- get_project
- list_routes
- reserve_port
- release_port
- get_preview_url
- cleanup_orphans

## Ownership-scoped ports

The MCP bridge generates an owner token.

reserve_port injects that token.

release_port injects the same token and the daemon refuses mismatched ownership.

Reservations currently remain in daemon memory with a 60-second TTL.

## Orphan cleanup policy

cleanup_orphans is intentionally dry-run only.

AgentDock can currently detect lifecycle orphaning, but that alone is not sufficient proof that terminating the underlying OS process is safe.

Destructive cleanup will require a durable AgentSession/process/worktree ownership graph.

## Safety boundary

No MCP tool can execute arbitrary shell commands or terminate arbitrary PIDs.
