# Mefi's Studio AI+ clean-room reverse-engineering brief

Status: reverse engineering started 2026-09-25  
Tracker: RE-243  
Issue: #12  
Source post: https://www.reddit.com/r/SideProject/s/NdWiBZGsNL  
Upstream: https://github.com/nateecho32-stack/mefi-studio  
Upstream reviewed head: 8fd7ac1a167e91fc29e6c36e4f0c9a5d79b2a8d5  
Upstream license: MIT  
Canonical destination: AgentDock

## Executive mapping

Mefi's Studio AI+ is not primarily useful to AgentDock as another desktop shell. Its strongest donor value is the operator/runtime contract around a team of coding agents:

- durable task/run/worker visibility
- attention routing for blocked or waiting work
- transactional task claims
- file-collision avoidance before parallel dispatch
- optional isolated Git worktrees
- evidence-gated completion and review
- retry/backoff and infrastructure-failure accounting
- replayable work events
- project/code context mapping
- model usage/cost surfaces

AgentDock already owns the safer local control-plane boundary: project/service discovery, coding-agent attribution, durable SQLite registry, owner-scoped resources, localhost API/MCP, stable local routing, and a fail-closed posture around process cleanup. RE-243 should therefore be a consolidated capability donor, not a new duplicate product.

## Sources reviewed

The reverse-engineering pass covered:

1. the exact Reddit launch thread and its current comments
2. upstream README
3. package metadata
4. architecture documentation
5. agent-loop/orchestration documentation
6. UX audit / proposed information architecture
7. changelog and current release state
8. current AgentDock README and existing safety/runtime boundary

### Release boundary

The public package currently identifies itself as 0.3.3. The launch post describes 0.4 as in development/unreleased. Treat behavior documented only for 0.4 as a public design target, not shipped parity.

## Reddit feedback incorporated

At review time the exact launch thread has one substantive comment. Its product signal is to prioritize what remains to be done and avoid adding more noise.

That should materially influence our implementation order:

1. Needs You / blockers
2. What's Left / Up Next
3. Review / verification
4. Live workers
5. richer maps/visualization later

The source's 3D presentation and ambient experience are optional parity, not a prerequisite for useful operator behavior.

## Source product model

### Workspace

- local-first desktop workspace
- one project folder is the active operating context
- tasks, plans, ideas/references, sessions and evidence are project-scoped
- UI can be closed while service-loop work continues
- interrupted work is intended to resume

### Agent/task runtime

- tasks move through a machine-managed pipeline
- service/foreman loop finds eligible work
- task claims are transactional and re-read fresh before activation
- active attempts receive run identity / lease-like ownership
- collaboration checks can defer work when sibling work or a live editor conflicts on likely files
- manual and automatic execution modes coexist

### Worktree model

When isolated runs are enabled, the source can:

- create a per-run worktree
- create a dedicated run branch
- execute the attempt in isolation
- merge successful work back serially
- preserve failed, dirty, crashed or otherwise inspectable work rather than blindly deleting it

This maps directly to AgentDock's planned durable session + worktree topology work.

### Verification model

Successful execution is not identical to done.

The source distinguishes:

- worker reported success
- evidence collected
- awaiting verification/review
- checks passed / user confirmed
- final completion

Verification commands are part of the settlement lifecycle. Recent source fixes also document secret-stripped verification environments.

### Retry / failure model

- retry uses backoff
- task failures are bounded
- infrastructure/start failures are tracked separately from logical task failure
- start failures should not silently turn a long-running/quiet worker into a failed task
- service restart/project switching has explicit recovery concerns

### Operator surfaces

The source's current UX direction converges on:

- Home
- Work
- Live
- Models
- Settings

The useful information architecture is operational rather than ornamental:

**Home**
- service state
- workers
- needs attention
- up next
- recent done
- machine
- usage
- project

**Work**
- queue
- review
- done
- ideas
- plans
- attempt/check/output details

**Live**
- worker/task topology
- current step
- readiness
- ranked queue
- optional constellation/tree/overhead visual modes

**Models**
- provider/model catalog
- project/provider usage and cost
- quota/account probes where available

### Project Map

The announced/project-map direction combines:

- Git history
- files read by agents
- files changed by agents
- task/run relationships
- project areas/components

The useful AgentDock interpretation is a provenance-backed project context graph, not merely a 3D visualization.

## Gap analysis against AgentDock

AgentDock already has:

- durable SQLite registry
- project/service identity
- agent ancestry attribution
- owner-scoped port/resource model
- local authenticated control plane
- MCP inspection/orchestration
- stable localhost routing
- fail-closed cleanup posture

RE-243 fills the next layer:

- durable agent sessions
- task attempts/runs
- normalized task state vocabulary
- worktree/branch topology
- event/log persistence
- attention and review projections
- evidence receipts
- verification settlement
- collision/claim gates
- replay
- project-context graph

## Proposed AgentDock architecture

### Registry additions

Add durable entities roughly equivalent to:

- `agent_session`
- `task`
- `task_attempt`
- `run_lease`
- `worktree`
- `file_claim`
- `event`
- `evidence_receipt`
- `verification_check`
- `attention_item`

Do not expose these as independent mutable UI stores. UI views should be projections of daemon-owned durable state.

### State vocabulary

Use one normalized state vocabulary across API, UI, CLI and MCP.

Task:
- queued
- active
- needs_input
- awaiting_verification
- done
- failed
- cancelled

Attempt/session can retain lower-level lifecycle states, but task-level UX must not invent alternate synonyms.

### Claiming and collaboration gate

Before dispatch:

1. read the candidate from durable storage
2. verify it is still eligible
3. inspect active sibling attempts
4. compare explicit or predicted file claims
5. check known live-editor ownership where available
6. claim transactionally
7. create attempt/run identity
8. only then launch work

On conflict, defer rather than race.

### Worktree topology

For an isolated attempt:

- project root remains canonical
- create owner-scoped worktree + branch
- bind worktree to task attempt/session
- persist topology before launch
- preserve dirty/failed/crashed worktrees for inspection
- merge successful branches serially
- record merge result as evidence/event
- require ownership proof for cleanup

### Evidence-gated settlement

A worker report is an input, not proof.

Settlement should capture:

- completion report
- changed files / diff identity
- test/check commands
- exit status
- relevant logs/artifacts
- verification environment policy
- user approval when required

Only then may the task transition from `awaiting_verification` to `done`.

### Retry/backoff

Persist:

- attempt number
- reason class
- retryable vs terminal
- next eligible time
- start/infra vs task failure
- circuit-breaker state where applicable

Do not consume logical task retries for quiet/long runs merely because output is sparse.

### Operator projections

Phase A UI should privilege the exact Reddit feedback:

**Needs You**
- blocked on user answer
- failed verification requiring decision
- ownership/conflict ambiguity
- merge/review conflict
- permission/provider issue

**What's Left / Up Next**
- remaining queued work
- rank/priority
- blockers/dependencies
- ready/not-ready reason
- active capacity

**Review**
- completed attempts awaiting checks or confirmation
- evidence receipts
- diff/check summary
- accept/retry/reopen paths

**Live**
- active session/task
- current phase/step
- elapsed/retry state
- owned worktree
- relevant logs/events

### Replay

Every material lifecycle transition should produce a durable event:

- task created
- claim attempted
- claim accepted/deferred
- session started
- worktree created
- file claim acquired/released
- output/check evidence
- needs-input
- retry scheduled
- verification started/completed
- merge result
- task settled

Replay is an audit projection over those events, not a separate recording system.

## Project Map / context graph

Phase B should build a deterministic graph from:

- repository paths
- Git commits/changes
- task/session/attempt links
- files read
- files changed
- verification/test files
- explicit project/module labels

Potential derived edges:

- task -> touched file
- task -> read file
- session -> task
- attempt -> branch/worktree
- file -> commit
- file -> module
- verification -> file/test
- task -> predecessor/dependency

Context recommendations for new work must carry provenance: why each file/module is suggested.

## Model/usage surfaces

Keep model telemetry bounded and provider-aware:

- provider/model
- session/task association
- token/cost when trustworthy
- quota/account probes only through supported adapters
- explicit unknown state when provider data is unavailable

Do not fabricate cross-provider comparability.

## Security and trust boundaries

RE-243 must not weaken AgentDock.

Required:

- owner/session isolation
- no arbitrary shell tool exposed merely to satisfy parity
- no generic unowned process kill
- localhost-first control surface
- explicit project-root confinement
- path traversal/symlink defense for file operations
- verification environment stripped of secrets unless explicitly allowlisted
- worktree cleanup only with ownership proof
- failed/dirty work preserved for inspection
- approval records bound to exact action parameters
- event/log redaction policy for secrets

## Implementation phases

### Phase A — runtime + operator contract

1. durable session/task/attempt/event schema
2. reconnect-safe lifecycle
3. worktree topology
4. claim/collision gate
5. evidence receipts + verification state
6. retry/backoff reason model
7. Needs You / What's Left / Review / Live projections
8. exact-head integration tests

### Phase B — Project Map

1. Git/file/task graph ingestion
2. read/change provenance
3. task-context ranking
4. inspectable why-this-context evidence
5. module/area projection

### Phase C — workflow acceleration

1. reusable task recipes
2. stuck-agent diagnostic worker
3. machine/usage status
4. provider quota/cost adapters
5. richer visualization if it improves operator decisions

## Phase A acceptance criteria

- two or more owned coding-agent sessions can run concurrently without task-claim races
- a colliding file claim defers one attempt rather than allowing ambiguous concurrent writes
- isolated worktree topology survives UI/daemon restart
- a successful worker report lands in `awaiting_verification`, not `done`
- verification evidence can transition the task to done
- failed verification can reopen/retry without losing prior evidence
- quiet/long-running agents are not killed solely for lack of recent text output
- Needs You and What's Left can be derived from daemon state after restart
- event replay reconstructs the material lifecycle of a run
- session A cannot mutate/stop session B without ownership authorization
- cleanup cannot delete an unowned or dirty worktree
- exact-head CI covers race, collision, reconnect, retry and verification settlement

## Explicit non-goals for first parity

- matching source 3D graphics
- ambient audio/music
- Discord/community/reward features
- reproducing the Electron shell
- hosted multi-tenant control plane
- broad autonomous destructive cleanup
- UI-only mock parity without durable runtime evidence

## Verification status

Public source behavior and architecture are sufficiently documented for a high-confidence donor map. The unreleased 0.4 UI/runtime behavior remains a design target until it is tagged/released. AgentDock currently has the correct canonical control-plane foundation, but RE-243 runtime parity is not yet implemented.

Next action: implement Phase A behind AgentDock's existing safety boundary and certify the exact branch/head before raising completion status.
