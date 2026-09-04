# ADR 0001 — Local-first Rust core

- Status: Accepted
- Date: 2026-09-04

## Context

AgentDock must inspect and eventually orchestrate OS processes, sockets, worktrees, and local reverse-proxy routes across macOS, Linux, and Windows.

The core must remain available even when the developer is offline and must not require application traffic to traverse AgentDock-operated cloud infrastructure.

## Decision

Use Rust for the local discovery/orchestration core and keep cloud services optional.

The MCP bridge and desktop UI may use TypeScript/React, but destructive policy and durable lifecycle logic belong in the Rust daemon.

## Consequences

Benefits:

- cross-platform native capabilities
- strong memory/thread safety
- single local policy boundary
- small deployable binaries
- cloud independence

Costs:

- some OS integrations require platform-specific code
- desktop/UI contributors cross a Rust/TypeScript boundary
