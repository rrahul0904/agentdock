# Pragma -> AgentDock clean-room reverse-engineering brief

Status: research/spec only  
Tracker candidate: RE-228  
Canonical destination: `rrahul0904/agentdock`  
Source: https://www.reddit.com/r/SideProject/s/S4elJPdsP3  
Product: https://pragma-app.sh/  
Upstream: https://github.com/pragma-sh/pragma  
Upstream license: AGPL-3.0

## 1. Why this belongs in AgentDock

Pragma is an agentic development environment centered on parallel coding agents, isolated Git worktrees, persistent sessions, attention state, review, fanout, automation, and remote/mobile control.

AgentDock already owns the lower-level local development control plane:

- project and service discovery
- process/port tracking
- worktree-aware project identity
- coding-agent attribution
- durable SQLite registry
- local HTTP/control API
- MCP bridge
- stable routes and owner-scoped reservations

Therefore this donor should extend AgentDock rather than create a competing standalone product.

## 2. Observed product surface

Publicly described behavior includes:

### Workspace and isolation
- many repositories in one workspace
- one isolated Git worktree/branch per task or agent
- nested worktrees for follow-up tasks
- persistent terminal sessions and scrollback
- SSH projects and Windows/WSL support

### Task lifecycle
- prompt/task board
- task assignment to an agent
- visible progress and attention state
- draft -> in progress -> review needed -> completed lifecycle
- PR number retained on the task/card

### Fanout
- one prompt sent to multiple agents/models
- isolated attempts
- common base commit
- side-by-side terminals/diffs/scratchpads
- explicit human selection of the preferred result
- agent-triggered fanout through CLI/SDK

### Git/GitHub
- staged/unstaged/committed diff review
- stage/commit/push in the workspace
- PR creation
- review threads/comments/check status
- review-item-to-agent fix workflow
- stacked PR awareness

### Extensibility
- plugin API
- provider/agent plugins
- command palette extensions
- sidebar/tool extensions
- TypeScript SDK
- CLI
- event/schedule automations

### Structured agent output
- scratchpads
- interactive MDX/React-style results
- Excalidraw whiteboards

### Remote control
- persistent host server
- localhost gateway exposed only through an explicit tunnel
- bearer-token remote access
- web client
- iOS/Android companion behavior
- remote response to approvals/questions

### Operational signals
- running / done / needs-attention state
- notifications
- provider quota/usage observations
- stale-worktree/storage-management need surfaced by users

## 3. Clean-room rules

The donor is AGPL-3.0. For the AgentDock implementation:

- do not copy upstream source files
- do not copy upstream UI assets, icons, names, screenshots, or copy
- do not reproduce unique implementation details unless independently derived
- use public behavior as requirements evidence
- implement against AgentDock's existing types, daemon, registry, API and MCP boundaries
- preserve AgentDock owner/session authorization and fail-closed behavior
- keep provider credentials outside durable task/run metadata
- preserve an explicit distinction between observed behavior and proposed AgentDock behavior

## 4. Proposed AgentDock domain model

### WorkspaceTask
- `task_id`
- `project_id`
- `title`
- `prompt_digest`
- `base_ref`
- `parent_task_id?`
- `state`
- `created_by_owner`
- `created_at`, `updated_at`

### WorktreeBinding
- `binding_id`
- `task_id`
- `worktree_path`
- `branch_name`
- `base_commit_sha`
- `parent_binding_id?`
- `lifecycle_state`
- `last_activity_at`

### AgentRun
- `run_id`
- `task_id`
- `binding_id`
- `agent_kind`
- `adapter_version`
- `state`
- `started_at`
- `finished_at?`
- `exit_summary?`
- `attention_required`
- `artifact_summary?`

### AttentionRequest
- `attention_id`
- `run_id`
- `kind` = approval | clarification | failure | review
- `status` = open | answered | dismissed
- `prompt_summary`
- `created_at`
- `resolved_at?`

### FanoutGroup
- `fanout_id`
- `task_id`
- `base_commit_sha`
- `run_ids[]`
- `comparison_state`
- `selected_run_id?`
- `selection_actor?`

No raw model secrets, provider tokens, hidden chain-of-thought, or arbitrary conversation payloads should be required for these core records.

## 5. Deterministic state machine

Allowed task/run progression should be explicit and reject invalid transitions.

Suggested task states:

`draft -> queued -> running -> needs_attention -> review -> completed`

Terminal alternatives:

`failed | cancelled`

Rules:
- idempotent duplicate events must not advance state twice
- a restart must rebuild current state from durable records
- stale/unknown are evidence states, not silently converted to completed
- cancellation must not auto-delete the worktree
- human approval is required before any destructive cleanup
- selecting a fanout result is separate from merging it

## 6. Provider-neutral adapter boundary

The first slice should expose a minimal adapter contract:

- identify agent kind/version
- launch in an existing AgentDock-owned worktree
- report process/session identity
- report lifecycle evidence
- surface attention requests
- cancel safely
- return bounded artifact/diff metadata

Adapters may initially cover two existing AgentDock-recognized coding-agent families using deterministic fixtures. Provider-specific quota APIs and proprietary account state are out of scope for Phase A.

## 7. Phase A implementation slice

The smallest useful implementation is:

1. migration/schema for WorkspaceTask, WorktreeBinding, AgentRun, AttentionRequest and FanoutGroup
2. task/run state reducer
3. provider-neutral launcher abstraction
4. on-demand fanout API/CLI:
   - create task
   - create N isolated child worktrees
   - launch N runs
   - persist common base SHA
   - report status
5. read-only task/run/worktree/attention endpoints and CLI
6. stale-worktree metadata only, with no default auto-delete
7. focused tests:
   - concurrent worktree isolation
   - parent/child lineage
   - idempotency
   - invalid transition rejection
   - cancellation
   - restart/recovery
   - owner isolation
   - no-secret persistence

## 8. Phase B

After Phase A is repository-certified:

- human comparison view contract
- diff/artifact summaries
- explicit selected-run state
- safe merge proposal with approval
- GitHub PR metadata integration
- review-item -> attention/fix task conversion
- quota observation adapters
- notification policy

## 9. Phase C / hosted or UI certification

Only after the local runtime is stable:

- persistent desktop host lifecycle
- browser/mobile remote client
- tunnel/token rotation
- plugin runtime
- automations scheduler/event bus
- scratchpad/whiteboard rich output
- embedded editor/media viewers
- production-grade GitHub PR review flows

## 10. Security and governance requirements

- bind local control surfaces to loopback by default
- remote access is explicit opt-in
- rotate/revoke bearer credentials
- no provider token persistence in task/run records
- owner-scoped task/run/worktree access
- fail closed on ambiguous ownership
- command/merge/delete operations require explicit authorization
- record provenance for agent adapter + base SHA + worktree + run
- cap fanout concurrency and resource use
- prevent path escape outside registered project roots
- never execute arbitrary plugin code without an explicit trust boundary

## 11. Evidence contract

Repository-level certification for Phase A requires:

- exact-head CI on the implementation commit
- migration tests
- state-machine tests
- concurrent worktree isolation test
- restart/recovery test
- at least two deterministic adapter fixtures
- API/CLI contract tests
- owner-isolation authorization tests
- no-secret persistence assertion

Hosted/mobile/plugin/GitHub parity stays unclaimed until separately demonstrated.

## 12. Mapping to existing AgentDock donor lineage

- AgentPort / remote control: control-plane lineage
- OpenTerm.app: local ADE/workspace UX donor
- Deiko: multimodal screen/voice context donor
- Lunavect: session presence, attention, activity and quota-observation donor
- **Pragma: parallel-agent task/worktree orchestration, fanout, durable attention workflow, extensibility and remote workspace donor**

The Pragma slice should reuse those foundations and must not weaken existing AgentDock session ownership, privacy, or fail-closed boundaries.
