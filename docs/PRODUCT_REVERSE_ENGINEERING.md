# Product Reverse Engineering Notes

## Inspiration

LocalDock validates a useful workflow abstraction: automatically discovered local services can be mapped to stable project names and selectively shared beyond the host machine.

The important mechanic to preserve is **project identity over port identity**.

## What we intentionally expand

AgentDock broadens the idea from a localhost convenience utility into an agent-aware development runtime:

1. process and port discovery
2. repository/worktree resolution
3. stable project identity
4. port reservation for concurrent agents
5. process ownership and agent attribution
6. lifecycle cleanup
7. local hostname proxying
8. MCP control surface
9. logs and health
10. browser verification
11. LAN/public preview adapters
12. team policy and audit capabilities

## Differentiation

The product moat should not be a tunnel implementation. Tunnels are commodity infrastructure and can be provided by adapters such as Cloudflare Tunnel.

The durable differentiation should be the local graph:

```text
AgentSession
  -> Task
  -> Repository
  -> Worktree
  -> Branch
  -> Process
  -> Port
  -> Service
  -> Preview
  -> VerificationResult
```

The more accurately AgentDock builds and maintains this graph, the more useful it becomes to both humans and agents.

## Competitive framing

- ngrok / Cloudflare Tunnel: connectivity
- local domain utilities: developer convenience
- process monitors: operating-system visibility
- IDEs: editing experience
- AgentDock: local agent runtime coordination

## Non-goal

Do not reproduce another product's proprietary code, assets, copy, or branding. Reverse engineering is used to understand workflow and market mechanics only.
