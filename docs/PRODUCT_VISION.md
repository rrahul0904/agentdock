# Product Vision

## Mission

Make local software development predictable when humans and multiple AI coding agents work concurrently.

## Product thesis

The unit of local development should be a **project/session identity**, not a port number.

Historically, a developer could reason about `localhost:3000`. In agentic development, many autonomous sessions may create servers, workers, browsers, containers, and worktrees simultaneously. Ports become ephemeral implementation details.

AgentDock introduces a control plane that understands:

- which services are running
- which repository/project each service belongs to
- which agent/session started it
- which worktree and branch it belongs to
- whether the process is healthy, stale, or orphaned
- how humans and agents can address it using a stable name
- how to safely expose or verify it

## North-star experience

A developer opens AgentDock and sees:

| Project | Agent | Branch | Port | Health | Stable URL |
|---|---|---|---:|---|---|
| storefront | Codex #2 | feature/cart | 3001 | healthy | storefront.localhost |
| api | Claude #1 | main | 8012 | healthy | api.localhost |
| admin | Cursor | feature/rbac | 5173 | building | admin.localhost |
| old-api | none | unknown | 8000 | orphaned | — |

An agent should be able to ask AgentDock for a free port, discover project services, open a stable preview, retrieve logs, run verification, and clean up only resources it owns.

## Initial target user

Individual developers and small engineering teams using Codex, Claude Code, Cursor, Gemini CLI, local terminals, Git worktrees, and containerized development.

## Product boundaries

AgentDock is not:

- a full Kubernetes replacement
- a cloud IDE
- a generic network tunnel company
- an observability warehouse
- an autonomous coding agent itself

AgentDock is the runtime coordination layer beneath coding agents.
