# AgentDock

**AgentDock is an AI local development control plane** for orchestrating projects, ports, processes, previews, worktrees, and coding agents.

The core idea is simple: local development should be addressed by **project identity**, not by ephemeral port numbers.

```text
Codex / Claude Code / Cursor / Terminal
                 |
                 v
          AgentDock Daemon
                 |
      +----------+-----------+
      |          |           |
   Projects    Ports      Processes
      |          |           |
      +----------+-----------+
                 |
                 v
        stable-name.localhost
```

## Why AgentDock

AI coding agents increasingly run several development servers at once. Ports change, stale processes survive, worktrees multiply, and humans lose track of which browser tab maps to which agent session.

AgentDock provides the local runtime control plane for that environment:

- discover listening development services
- resolve services back to repositories/projects
- classify frameworks and infrastructure
- distinguish development listeners from obvious OS noise
- assign stable project identities
- reserve ports safely for parallel agents
- attribute processes to agents and worktrees
- expose an MCP interface
- detect and clean orphaned services
- route stable `*.localhost` names
- add secure LAN/public previews later
- automate browser verification later

## Current status

**Phase 1 — reliable local discovery.**

Implemented:

- cross-platform Rust workspace
- macOS/Linux `lsof` discovery with PID, bind address, command, cwd, and command line enrichment
- Windows PowerShell listener/PID discovery
- project/repository resolution
- framework/runtime classification
- system-service classification with `--all` override
- Docker/Podman proxy recognition
- fixture-driven parser tests
- in-memory race-aware port reservation manager
- CLI scanner
- MCP bootstrap contract
- GitHub Actions cross-platform Rust CI

## Repository layout

```text
crates/
  agentdock-core/
  process-discovery/
  project-resolver/
  framework-detection/
  port-manager/
  agentdock-cli/

packages/
  mcp-server/

docs/
  PRODUCT_VISION.md
  PRODUCT_REVERSE_ENGINEERING.md
  ARCHITECTURE.md
  IMPLEMENTATION_PLAN.md
  PHASE_1_DISCOVERY.md
  SECURITY_MODEL.md
  MCP_DESIGN.md
  ROADMAP.md
  TESTING.md
  ADR/
```

## Run the Rust CLI

Prerequisites: Rust 1.80+.

```bash
cargo run -p agentdock-cli -- scan
```

Include system/unknown listeners:

```bash
cargo run -p agentdock-cli -- scan --all
```

JSON output:

```bash
cargo run -p agentdock-cli -- scan --json --all
```

Environment diagnostics:

```bash
cargo run -p agentdock-cli -- doctor
```

## Development principles

1. Local-first: core orchestration works without cloud services.
2. Least privilege: only expose a service when explicitly requested.
3. Project identity over port identity.
4. Agent-aware lifecycle management.
5. Cross-platform core: macOS, Linux, Windows/WSL.
6. Cloud adapters are optional, not architectural dependencies.
7. Every destructive action must be attributable and auditable.

## Product direction

AgentDock is inspired by the workflow problem highlighted by tools such as LocalDock, but is intentionally designed as a broader **agentic development runtime** rather than a clone.

See [docs/PRODUCT_VISION.md](docs/PRODUCT_VISION.md) and [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md).

## License

MIT
