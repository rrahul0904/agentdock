# MCP Design

## Principle

MCP should expose safe domain operations, not shell access.

## Planned tools

### list_services

Read-only inventory of discovered services.

Output fields:

- service_id
- project
- worktree
- agent_session
- pid
- port
- health
- stable_url

### reserve_port

Reserve an available port for a short bounded interval.

Inputs:

- owner/session
- optional preferred range

### release_port

Release a reservation owned by the caller/session.

### get_project

Return project/worktree/service topology.

### get_logs

Return bounded logs for a known service. Never expose arbitrary filesystem reads.

### get_preview_url

Return an already-authorized local/LAN/public preview URL.

### cleanup_orphans

Terminate only processes classified as orphaned and allowed by policy.

## Future verification tools

- verify_project
- get_verification
- capture_preview

## Protocol architecture

```text
AI client
   |
stdio MCP
   |
@agentdock/mcp-server
   |
versioned local AgentDock API
   |
agentdockd
```

The MCP bridge intentionally remains thin. Authorization and lifecycle policy live in the daemon so every client receives identical protections.
