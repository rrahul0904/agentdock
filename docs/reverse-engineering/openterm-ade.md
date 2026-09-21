# OpenTerm.app clean-room reverse-engineering brief

Status: reverse engineering started 2026-09-21  
Tracker: RE-205  
Source: https://www.reddit.com/r/saasbuild/s/4GCLW8KVGr  
Canonical destination: AgentDock

## Why this belongs in AgentDock

OpenTerm is positioned as an Agent Development Environment (ADE), not a traditional IDE: an agent-first local workspace for CLI coding agents such as Claude Code, Codex, and Grok Build.

That overlaps strongly with AgentDock's existing local control-plane responsibilities:
- coding-agent ancestry / attribution
- durable project and service registry
- stable local routing
- MCP bridge
- planned durable agent sessions
- planned worktree topology
- planned log capture and health checks

We should absorb OpenTerm as a workspace/operator UX donor rather than create another standalone product.

## Observed product promises

From the public launch post and follow-up:
- keyboard-first interaction
- multiple CLI coding-agent workflows organized in one workspace
- precise speech-to-text input
- advanced notifications
- basic IDE functions such as opening/editing files
- agent-first workflow rather than editor-first workflow

The author says source code is expected on 2026-09-23. Until the source and license are reviewed, implementation must remain clean-room and based only on observed behavior/public descriptions.

## Product surface to reproduce

### P0 — Workspace shell
- project/workspace selector
- one or more terminal sessions per workspace
- launch presets for Claude Code, Codex, Grok Build, custom shell
- persistent session metadata in AgentDock registry
- keyboard navigation between workspaces/sessions
- agent status: idle / running / waiting / failed / stopped
- attention badge and OS notification when an agent needs input

### P1 — Agent session lifecycle
- daemon-owned session IDs
- process ancestry attribution
- reconnect after UI restart without losing registry state
- per-session working directory, branch/worktree, command, timestamps
- log/event stream persisted to SQLite
- bounded stop/restart controls with ownership proof

### P1 — File sidecar
- file tree scoped to current project/session
- read-only preview first
- explicit edit mode with save confirmation
- Markdown rendering
- diff preview before save where practical

### P1 — Speech input
- push-to-talk / keyboard shortcut
- local-first speech-to-text where possible
- transcript preview before injection
- explicit target terminal
- never execute automatically after transcription

### P1 — Notifications
- agent waiting for input
- task completed
- process failed
- configurable per-workspace notification settings
- quiet/focus behavior without losing event history

### P2 — Workspace ergonomics
- terminal tiling/splits
- saved workspace layouts
- recent projects
- command palette
- configurable shortcuts
- themes
- session search

## Security boundaries

The OpenTerm donor must not weaken AgentDock's existing fail-closed posture.

Required:
- localhost-only control surface by default
- no arbitrary remote shell exposure
- explicit session ownership
- replay/session-fixation protection for any paired remote controls
- confirmation/approval records bound to exact command/session parameters
- terminal input injection must be explicit and target-scoped
- speech transcription must never auto-submit a command
- file writes must remain project-root scoped
- symlink/path traversal checks for file operations
- secrets/redaction policy for logs and event history

## Proposed architecture

### Existing AgentDock layers to reuse
- `agentdockd`: source of truth for sessions, projects, events
- registry: SQLite persistence
- agent attribution: provider/process identification
- proxy/control API: local authenticated API surface
- MCP server: inspection and safe orchestration

### New workspace layer
`packages/ade-ui/`
- workspace shell
- terminal pane model
- file panel
- activity/notification center
- keyboard command palette

`crates/agent-session/`
- durable session model
- lifecycle transitions
- reconnect metadata
- event/log persistence

`crates/terminal-bridge/`
- PTY lifecycle
- bounded input/output stream
- resize handling
- provider launch presets

`crates/workspace-files/`
- root-scoped browse/read
- later: guarded writes + diff

`crates/notification-engine/`
- waiting/completed/error transitions
- desktop notification adapter

Speech-to-text should be an adapter boundary rather than embedded into terminal transport.

## First implementation slice

1. Durable agent-session schema and lifecycle in the daemon.
2. PTY-backed terminal bridge tied to a verified project + session owner.
3. Minimal local workspace UI with:
   - workspace list
   - terminal pane
   - session status
   - activity badge
4. Claude Code + Codex launch presets.
5. waiting/completed notification events.
6. basic read-only file tree.
7. tests for:
   - session ownership
   - restart/reconnect metadata
   - path-root enforcement
   - terminal targeting
   - notification state transitions

## Acceptance criteria for the first slice

- two AgentDock workspaces can each run an independent coding-agent terminal
- UI restart does not erase durable session metadata
- session A cannot inject input into session B without owning/targeting it
- file browser cannot escape the project root
- waiting/completed transitions create durable events
- Codex and Claude Code sessions are correctly attributed where process ancestry supports it
- all behavior remains localhost/fail-closed by default

## Out of scope for the first slice

- cloud sync
- hosted shell execution
- collaborative multiplayer terminals
- remote command execution
- auto-executing voice commands
- broad arbitrary process kill
- copying any unpublished/proprietary OpenTerm implementation

## Follow-up once source is published

On/after 2026-09-23:
1. identify the official repository linked by OpenTerm
2. verify the license before reading code for reusable implementation details
3. compare public architecture against this clean-room design
4. record only defensible reusable ideas and explicitly track any incompatible/licensed components
