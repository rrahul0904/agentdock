# AgentPort donor analysis: browser-to-local coding agent control

Source observed on 2026-09-20:

- Reddit: https://www.reddit.com/r/SideProject/comments/1wl6icq/i_built_a_browser_chat_that_lets_an_ai_code_on_my/
- Product: https://agentport.johnstech.in/

## Observed product promise

The referenced AgentPort product lets a user open a browser chat from another device, connect a Mac or PC, and drive an AI coding agent against the real local project environment instead of a hosted sandbox.

The Reddit author explicitly describes access to the local project folder, packages, tools, and existing logins, with the user watching and approving when needed. The current post says Grok Build is supported first, with more models planned.

## Why this belongs in AgentDock

AgentDock already owns the local-development control-plane responsibilities that a safe implementation needs:

- cross-platform service discovery
- project and framework resolution
- coding-agent ancestry attribution
- durable local registry
- stable project/service identities
- localhost routing
- owner-scoped resource reservations
- daemon-backed MCP access
- fail-closed boundaries around destructive process control

Creating a separate product would duplicate the hardest local-control primitives. The donor should instead extend AgentDock with a remote trust and transport layer.

## Product gap exposed by the donor

AgentDock can observe and coordinate local development services, but it does not yet provide a user-facing browser/device control surface for a coding-agent session.

The missing end-to-end path is:

1. Pair a browser/device to one AgentDock installation.
2. Authenticate the paired device without exposing local credentials.
3. Select a known AgentDock project and attributed coding-agent session.
4. Stream bounded session status/log events.
5. Send agent input only to an identified coding-agent session.
6. Require an explicit approval decision for mutating actions.
7. Produce an audit event for pairing, input, approval, denial, and disconnect.
8. Revoke a paired device immediately.
9. Keep the daemon loopback-only by default.
10. Add remote transport without ever turning it into a generic shell endpoint.

## Trust boundary

The remote surface is not allowed to introduce:

- arbitrary shell execution
- arbitrary process kill
- unrestricted filesystem browsing
- raw local credential export
- approval-policy changes by the agent itself
- a non-loopback daemon bind as the default remote-access mechanism

Remote access must be an authenticated transport terminating in a narrow AgentDock capability layer.

## Capability contract

Phase 1 defines only four remote capabilities:

| Capability | Read-only | Pairing required | Explicit approval |
| --- | --- | --- | --- |
| Session inventory | yes | yes | no |
| Session logs | yes | yes | no |
| Agent input | no | yes | yes |
| Action approval | no | yes | yes |

This is implemented in `agentdock-core::remote`.

## Inferred architecture

```text
Browser / phone
      |
      | authenticated paired session
      v
Remote transport / relay
      |
      | opaque session identity + bounded capability request
      v
AgentDock remote gateway
      |
      +--> approval policy / audit log
      |
      +--> AgentDock durable agent session registry
      |
      +--> session log stream
      |
      +--> bounded agent input adapter
                  |
                  +--> Codex
                  +--> Claude Code
                  +--> Cursor
                  +--> Gemini CLI
                  +--> additional adapters later
```

A relay may be introduced later, but the relay must not become the authority for local execution. AgentDock remains the local policy and ownership boundary.

## Implementation status

Repository state on this branch:

- Slice A — implemented: fail-closed remote capability contract and local discovery endpoint.
- Slice B — implemented: durable coding-agent sessions, lifecycle state, bounded durable lifecycle logs, local API/CLI/MCP read paths.
- Slice C — implemented at the local trust boundary: short-lived one-time pairing challenges, hash-only secret persistence, invalid-secret lockout, pending requests, explicit loopback-only approve/deny, durable paired devices, revocation, and audit events.
- Slice D — not implemented: no remote relay or bidirectional transport is enabled.
- Slice E — not implemented: no remote agent-input adapter or remote approval execution path is enabled.
- Slice F — not implemented: no hosted browser control UI is claimed.

Pairing administration intentionally remains outside the MCP tool surface so a coding agent cannot approve or revoke its own remote access.

## Smallest implementation sequence

### Slice A — capability and fail-closed API contract

- Define the allowed remote capabilities in `agentdock-core`.
- Add a daemon endpoint that reports the contract while remote control remains disabled.
- Tests prove all remote capabilities require pairing and all mutating capabilities require approval.

### Slice B — durable agent sessions

- Persist a first-class coding-agent session separate from a discovered service.
- Bind the session to project, process ancestry, agent kind, lifecycle, and ownership.
- Add bounded log/status retrieval.

### Slice C — local pairing

- One-time pairing challenge generated locally.
- Short expiry.
- Device public-key or equivalent strong identity.
- Explicit local confirmation.
- Revocation ledger.
- No reusable plaintext pairing secret at rest.

### Slice D — transport

- WebSocket or comparable bidirectional channel.
- Authenticated session handshake.
- Server-side capability checks.
- Backpressure and reconnect semantics.
- Relay optional; direct/Tailscale-style transport can be supported separately.

### Slice E — bounded input + approval

- Input adapter targets a known coding-agent session, never an arbitrary terminal.
- Mutating requests create approval records.
- Approval is bound to exact action parameters.
- Approval cannot be broadened or replayed for a different action.

### Slice F — browser UI

- machine/project/session picker
- live status and logs
- pending approvals
- approve/deny
- text input to supported coding-agent sessions
- disconnect/revoke
- audit history

## Competitive/donor distinction

There are other products currently using the AgentPort name, including an MCP integration gateway and an iOS/tmux remote monitor. They are useful adjacent references but are not the Reddit product under analysis. This reverse-engineering slice is scoped to the browser-to-local-coding-agent product linked in the Reddit post.

## Exit criteria before calling the feature production-ready

- remote daemon exposure is not required
- pair/revoke is authenticated and tested
- cross-device reconnect is tested
- every mutating action is parameter-bound to an approval
- audit events are durable
- unsupported agent/runtime combinations fail closed
- no generic shell endpoint exists
- local credentials never traverse the browser/relay
- threat-model tests cover replay, stolen pairing code, session fixation, privilege expansion, and revoked devices
- CI passes on Linux, macOS, and Windows
- hosted/browser verification proves the complete pairing -> observe -> approve -> input -> revoke path
