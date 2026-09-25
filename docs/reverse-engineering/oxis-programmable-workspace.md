# OXIS clean-room / source-audited reverse-engineering brief

Status: reverse engineering started 2026-09-25  
Tracker: RE-243  
Primary source: https://www.reddit.com/r/coolgithubprojects/s/8hRtSjN8Tn  
Upstream: https://github.com/oxlaboratory/oxis  
Upstream license: Apache-2.0  
Canonical destination: AgentDock  
Relationship: complementary capability donor to RE-205 OpenTerm.app

## 1. Scope and evidence policy

This brief separates three things:

1. **Observed / source-verified OXIS behavior** from the public launch thread, current public repository, README, changelog, CI and selected source files.
2. **Community feedback** from the Reddit discussion.
3. **AgentDock implementation requirements** derived from those observations.

Do not infer the private Stripe/payment backend or premium plugin sources: the OXIS README explicitly says those are private and not in the public repository.

Because OXIS is Apache-2.0 while AgentDock is MIT, default implementation should be clean-room/spec-driven. If source is copied or adapted later, preserve the required Apache license/NOTICE obligations and record provenance explicitly rather than silently relicensing code as MIT.

## 2. Why this belongs in AgentDock

OXIS is not just a shell skin. Its product surface is a local developer control environment combining:

- a real PowerShell/bash PTY
- one global command prompt
- a built-in editor
- named/project workspaces
- automatic project/task discovery
- tasks and workflows
- git project linking
- Lua automation and lifecycle events
- per-plugin permissions and dependency manifests
- a plugin market
- backup/restore, diagnostics and self-update

AgentDock already owns the lower-level local developer control plane:

- project/framework discovery
- process and coding-agent attribution
- durable SQLite registry
- local routing and loopback control API
- owner-scoped port reservations
- MCP inspection/orchestration
- a fail-closed posture around process ownership

RE-205 OpenTerm.app already defines an agent-first workspace UI, PTY/session lifecycle, file sidecar, notifications and voice input. OXIS should therefore extend that lineage rather than create a second local-control product.

## 3. Upstream product model

### 3.1 Command dispatch

OXIS preserves the user's normal shell. Ordinary input goes to PowerShell/bash. OXIS-owned commands are explicitly prefixed with an apostrophe (or `oxi `).

AgentDock implication:
- preserve a visible distinction between raw terminal input and control-plane commands
- never ambiguously reinterpret arbitrary shell text as an AgentDock action
- command palette actions should resolve to typed operations, not hidden shell-string rewriting

### 3.2 Global prompt

The current OXIS design uses one prompt at the bottom of Home, terminal and editor with:
- shared history
- prefix-filtered history navigation
- reverse search
- readline-style keybindings
- selection/copy behavior that does not accidentally interrupt a process

AgentDock implication:
- the ADE UI should have one explicit command/input targeting model
- input must always expose its target session/workspace
- copy/selection must not be conflated with process signals

### 3.3 Terminal

The OXIS backend uses a native PTY:
- ConPTY on Windows
- creack/pty on Linux
- local WebSocket transport for terminal I/O
- loopback HTTP server
- origin restriction for browser WebSocket access

The public changelog records a recent security fix because the PTY WebSocket previously accepted connections from arbitrary websites. The current server checks loopback Host and allowed Origin.

Known upstream limitation:
- full-screen terminal programs such as vim/htop are not supported because output is normalized into OXIS's own line model.

AgentDock implication:
- preserve AgentDock's ownership/session boundaries in addition to origin checks
- terminal transport needs explicit session IDs, owner tokens and anti-cross-session input rules
- prefer full VT/PTY fidelity rather than adopting OXIS's plain-line limitation unless deliberately scoped

### 3.4 Built-in editor

Observed surface:
- tabs
- file tree
- save/undo/redo
- find/replace and go-to-line
- Vim-style normal/insert/visual modes
- syntax highlighting for common developer formats
- change gutter
- live HTML/Markdown preview, including Mermaid
- preview sandboxing
- large-file highlighting cutoff
- hot reload when saving a plugin Lua file

AgentDock implication:
- RE-205's read-only file sidecar can evolve into guarded editing
- file writes remain rooted to the verified project/worktree
- add diff-before-save for externally meaningful edits
- do not let preview content reach the control API/terminal bridge

### 3.5 Workspaces and project linking

OXIS supports:
- folder-local `.oxis/workspace.lua`
- named workspaces with documents/plugins/scripts/tasks/workflows
- linking a real project directory
- root-scoped file operations
- a connector marker
- automatic task detection for Rust, Node, Python, Go, .NET, Java, C/C++, PHP and Ruby
- automatic reconciliation that does not overwrite user-edited tasks

AgentDock implication:
- use AgentDock's project registry as source of truth
- store workspace/session layout separately from repository files by default
- allow optional project-local declarative config
- task discovery should produce proposed/generated definitions with provenance and "user-modified" protection

### 3.6 Git integration

OXIS uses real git processes with argument arrays rather than composing one shell string. Its task commit flow stages, commits, pushes, reports common failures and supports cancellation. GitHub/GitLab remote linking is workspace-aware.

AgentDock implication:
- git actions should be bounded typed operations
- expose cwd/repo/worktree/branch in every action
- never allow an AgentDock session to mutate another session's worktree implicitly
- retain command/result provenance for agent-driven operations

### 3.7 Tasks and workflows

OXIS exposes named tasks and workflows. Workflow steps can:
- invoke a task
- run a shell command
- invoke an OXIS command
- retry
- continue on error
- run conditionally
- group command steps as parallel

Its own README notes shell steps are effectively serialized because there is one shell.

AgentDock implication:
- create an explicit workflow DAG/step model rather than overloading one terminal
- every step should declare workspace/session/cwd/environment/permissions
- concurrency should be owner- and resource-aware
- durable execution records should survive UI restart
- approval boundaries belong on mutation classes, not string matching

## 4. Lua plugin/runtime model

OXIS plugins run in separate embedded Lua 5.3 VMs via Fengari.

Plugin manifests can declare:
- version
- description
- author
- category
- minimum OXIS version
- operating system
- permissions
- dependencies and version ranges

Compatibility checks cover:
- OS
- minimum host version
- missing dependencies
- incompatible versions
- dependency cycles

### Permission namespaces

Observed permission-gated capabilities include:
- filesystem
- process
- network
- system
- workspace
- terminal
- shell execution

Legacy/unmanifested plugins ask for permissions at runtime. Manifested plugins cannot exceed declared permissions. Shell access is treated specially and prompts.

Important limitation from the public README:
- plugins have separate Lua VMs, but are **not fully sandboxed from each other beyond that**.

AgentDock implication:
- do not copy the trust model literally
- treat extension permissions as capability tokens attached to the exact extension + workspace + owner
- isolate extension storage and secrets
- mutation-capable tools require an auditable approval policy
- extension failure must not corrupt daemon/session state

## 5. Plugin market and supply-chain model

Observed public Market:
- static index plus Lua plugin files
- browse/search/info/install/update
- compatibility checks
- rollback
- publishing via GitHub pull request

### Reddit feedback that must be preserved

A commenter identified a concrete rollback failure mode: the older `saveBackup()` path swallowed storage errors, while `market update` continued. If the new plugin then failed, rollback could have no usable backup.

The upstream commit history includes a commit explicitly describing a fix for the `saveBackup()` failure/update-continues bug. This means the comment was actionable and appears to have been addressed upstream; it still defines a **required fail-closed invariant** for our rebuild.

Required invariant:
> Never begin a destructive/update mutation unless the rollback artifact/state has been durably written and verified.

A second discussion proposed approval checkpoints around state-changing plugin/market commands while keeping read-only listing/lookups automatic.

AgentDock requirement:
- read-only: catalog/list/search/info/validate can be automatic
- mutation: install/update/enable/disable/uninstall/publish requires a typed policy decision
- for agent-driven actions, emit a review record containing exact extension, source, version/hash, requested capabilities, workspace and intended state transition
- approval cannot be reused for a materially different version/hash/action

## 6. Backup, restore and self-update

OXIS supports:
- JSON backup/restore for settings, named workspaces, created documents/plugins
- diagnostics with local state and recent errors
- no telemetry claim in its public docs
- commit-based update detection
- in-place binary replacement with executable backup and restore on startup failure

AgentDock implication:
- configuration/state migrations need versioned durable backups
- updater must not own application/session recovery semantics
- rollback verification must be testable
- signed/reproducible distribution should be a future release gate if AgentDock ships a desktop binary

## 7. Monetization surface — observed, not a Phase A requirement

OXIS documents planned premium plugin subscriptions:
- Stripe subscription flow
- 75% developer / 25% OXIS share for third-party plugins
- premium source delivered after license check
- AES-256-GCM encrypted local package using a device-derived key
- license re-check on load
- AI DevOps premium plugin using an OpenAI-compatible endpoint and the user's key

Current public status:
- premium listings are marked coming soon
- payments remain in Stripe test mode
- private payment backend/premium sources are not public

AgentDock decision:
- do **not** add marketplace monetization to the first OXIS donor slice
- first build the extension contract, provenance, capability controls and rollback invariants

## 8. Proposed AgentDock architecture

Reuse:
- `agentdockd` — authoritative projects, sessions, events
- SQLite registry
- agent-attribution
- local authenticated control API
- existing MCP server
- RE-205 workspace/ADE design

Add or extend:

### `packages/ade-ui/`
- workspace/project selector
- terminal pane(s)
- global command/agent input surface
- file sidecar/editor
- task/workflow panel
- extensions panel
- activity/approval center

### `crates/agent-session/`
- durable PTY session metadata
- owner/session binding
- status transitions
- reconnect semantics
- process ancestry and agent attribution
- event/log pointers

### `crates/terminal-bridge/`
- PTY lifecycle
- session-scoped input/output
- resize
- signal handling
- explicit shell vs AgentDock-action dispatch

### `crates/workspace-files/`
- rooted list/read/stat
- guarded writes
- path/symlink traversal defense
- diff/approval metadata

### `crates/task-runtime/`
- detected task definitions with provenance
- user-edited protection
- typed task execution
- cancellation
- environment and cwd scoping

### `crates/workflow-runtime/`
- durable workflow/step state
- retry/continue-on-error/conditions
- concurrency model
- approval pauses
- replay-safe continuation

### `crates/extension-runtime/`
- manifest schema
- version/dependency validation
- capability grants
- extension lifecycle
- host API boundary
- isolated extension state

### `crates/extension-registry/`
- source URL
- package/version/hash
- install/update history
- verified rollback artifact
- enable/disable state
- provenance/audit record

## 9. Implementation phases

### Phase A — programmable workspace foundation

1. Durable PTY session model on the existing AgentDock daemon.
2. Workspace UI terminal pane with explicit target session.
3. Root-scoped read-only project file tree.
4. Detected task inventory for Node/Python/Go/Rust first.
5. Typed task execution through an owned session.
6. Durable task/session events.
7. Tests for ownership, path containment, reconnect and cancellation.

Acceptance:
- two workspaces can run independent sessions
- one session cannot inject input into another
- UI restart preserves session metadata
- file tree cannot escape project root through `..` or symlink traversal
- detected tasks show source/provenance
- task execution records cwd, owner, session and result

### Phase B — editor + workflow layer

1. Guarded file editing with diff-before-save.
2. Workspace-local declarative config.
3. Durable workflows with task/run/action steps.
4. retry, continue-on-error and conditional steps.
5. explicit parallelism based on separate owned sessions/resources.
6. approval pause/resume records.

Acceptance:
- a workflow resumes from durable state without replaying completed mutations
- changed parameters invalidate old approvals
- concurrent steps cannot steal another step's session/port/worktree
- edited generated tasks are not silently overwritten

### Phase C — extension runtime

1. Extension manifest + version/dependency model.
2. Read-only extension catalog and validation.
3. Capability-scoped host API.
4. install/update/enable/disable lifecycle.
5. verified pre-update snapshot.
6. automatic rollback on load/validation failure.
7. audit and approval records.

Acceptance:
- update aborts if rollback snapshot cannot be durably verified
- extension cannot call undeclared capabilities
- extension A cannot access extension B state/secrets
- update/install binds approval to exact source/version/hash
- failed extension cannot corrupt daemon/session registry

### Phase D — optional marketplace/distribution

Only after Phase C security certification:
- signed package metadata
- source/provenance display
- publisher verification policy
- update channels
- revocation
- optional monetization evaluation

## 10. Security gates

P0:
- localhost-only by default
- Host + Origin validation for browser transports
- authenticated owner/session token on state-changing control calls
- no arbitrary cross-session input
- no broad process-kill endpoint
- rooted file operations with canonical path + symlink checks
- secrets redaction in logs/events
- explicit terminal target for pasted/transcribed input
- approvals bound to exact action parameters

P0 extension invariants:
- hash every installable artifact
- fail closed if snapshot/backup creation fails
- validate before activation
- rollback on activation failure
- preserve previous known-good artifact until new version is certified
- no auto-enable of untrusted dependency without policy evaluation
- record source, version, hash, permissions and result

## 11. Test matrix

Terminal:
- Windows/Linux PTY startup
- resize
- UTF-8 split reads
- Ctrl+C / EOF handling
- reconnect
- high-output streams
- interactive command behavior

Transport:
- non-loopback Host rejected
- hostile browser Origin rejected
- wrong session owner rejected
- replayed/stale token rejected where tokens are used

Files:
- `..` traversal
- symlink escape
- deleted/moved roots
- large files
- binary files
- concurrent edit conflict

Tasks/workflows:
- detection fixtures per ecosystem
- cancellation
- retry
- conditional skip
- durable resume
- partial failure
- concurrency/resource collision

Extensions:
- missing dependency
- version conflict
- cycle
- undeclared capability
- denied capability
- failed backup
- corrupt backup
- failed activation
- rollback verification
- malicious source/path fields
- approval mismatch after version/hash changes

## 12. Explicit non-goals for the first implementation

- copying OXIS UI pixel-for-pixel
- reproducing Stripe/premium infrastructure
- importing private OXIS backend behavior
- arbitrary remote shell exposure
- auto-executing voice input
- broad process termination
- claiming extension sandbox parity before isolation tests exist
- claiming production readiness from unit tests alone

## 13. Evidence map

Primary:
- Reddit launch/comments: https://www.reddit.com/r/coolgithubprojects/s/8hRtSjN8Tn
- OXIS repo: https://github.com/oxlaboratory/oxis
- README: https://github.com/oxlaboratory/oxis/blob/main/README.md
- CHANGELOG: https://github.com/oxlaboratory/oxis/blob/main/CHANGELOG.md
- Market client: https://github.com/oxlaboratory/oxis/blob/main/frontend/src/plugins/market.ts
- Plugin manager: https://github.com/oxlaboratory/oxis/blob/main/frontend/src/plugins/pluginManager.ts
- Local server: https://github.com/oxlaboratory/oxis/blob/main/internal/server/server.go
- CI: https://github.com/oxlaboratory/oxis/blob/main/.github/workflows/build.yml
- Reddit-feedback fix commit: https://github.com/oxlaboratory/oxis/commit/4cb96bed2962807ee7124655219434a8a814a38f
- Reliability/security pass: https://github.com/oxlaboratory/oxis/commit/027fcb7951ab3ca68bf15445706e9d95c3f02ed3
- PTY origin restriction: https://github.com/oxlaboratory/oxis/commit/fa1c1e9c3ba205b4068b37122dc65b5a1f0c081e

AgentDock related:
- RE-205 OpenTerm brief: `docs/reverse-engineering/openterm-ade.md` on `reverse/openterm-ade`

## 14. Current status

Reverse engineering is evidence-complete enough to begin implementation planning. No OXIS parity implementation is claimed yet.

Next concrete action:
- implement Phase A on this lineage or a dedicated implementation branch from the current AgentDock base
- keep OXIS and OpenTerm requirements separate in provenance, but converge them into the same AgentDock ADE/runtime architecture
