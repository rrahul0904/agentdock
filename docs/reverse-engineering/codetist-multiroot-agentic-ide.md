# RE-361 — Swavoti / Codetist IDE research dossier

Date: 2026-10-01

## Intake

Primary supplied source:

- https://www.reddit.com/r/saasbuild/comments/1wusk9h/complete_your_coding_tasks_faster_introducing/

Additional public evidence used:

- Same-author crosspost: https://www.reddit.com/r/AppBuilding/comments/1wurjzn/introducing_swavoticodetist_ide/
- Product landing page: https://codetist.swavoti.co.za/main
- Cursor multi-agent documentation/changelog as a public comparator
- GitHub Copilot coding-agent documentation as a public comparator
- AgentDock issue #8 (Pragma donor) for portfolio deduplication

## What is publicly observable or author-reported

The launch describes Codetist as an IDE in which coding work can be handled by multiple/sub-parallel agents. It says the agent can edit, debug and fix workspace issues and highlights multi-folder indexation for large codebases/monorepos. The same-author crosspost describes multiple iterative agent loops running in parallel in dedicated isolated contexts and claims Linux and Windows availability.

These are public product/author claims. They are not treated as proof of internal architecture, performance, isolation quality, indexing completeness or platform certification.

No public Codetist implementation repository was identified during this intake. That means this effort is behavior-led clean-room research rather than source-based porting.

## Reddit feedback

The exact supplied thread exposes one visible independent comment challenging the "first agentic IDE" positioning because tools with similar capabilities already exist. The OP replies that the intended distinction is that the agent has access to the whole IDE.

That feedback changes the product plan:

1. Do not repeat "first agentic IDE" as a product claim.
2. Do not use generic parallel-agent support as the differentiator.
3. Make the wedge measurable: deterministic multi-root/monorepo scope, bounded indexing, isolation, and evidence.
4. Interpret "whole IDE access" as explicit capabilities rather than blanket authority.
5. Require approval/receipts for mutation and execution capabilities.
6. Show ownership, wait reasons and collision boundaries to the operator.

The accessible exact-thread comments do not name another project to add as a child tracker entry.

## Public comparator findings

Parallel or multi-agent coding is already publicly documented elsewhere. Cursor documents multi-agent coding and has described parallel agents using isolated codebase copies/worktrees or remote machines. GitHub documents coding agents that can work from repositories and create pull-request-based changes.

The implication for RE-361 is not that Codetist lacks value; it is that a clean-room rebuild should demonstrate a concrete operational advantage rather than rely on novelty language.

## Portfolio placement

Canonical destination: **AgentDock**.

RE-284 / AgentDock issue #8 already owns the Pragma-derived parallel-agent workspace direction: projects, tasks, worktrees, agent runs, fanout, attention state and later remote/operator surfaces.

AgentDock already owns:
- cross-platform project resolution;
- coding-agent attribution;
- durable local registry;
- stable local project/service identities;
- daemon + loopback control API;
- owner-scoped resource control;
- planned durable session/worktree topology.

Therefore Codetist is an incremental capability donor, not a standalone canonical product.

### Incremental donor delta

- multi-root / multi-folder workspace identity;
- deterministic workspace-root validation and limits;
- later bounded indexing/search across roots;
- explicit IDE capability grants for read/search/edit/test/debug/terminal;
- later binding of those roots to RE-284 task/worktree fanout.

## Phase A implemented in this branch

### Versioned workspace contracts

`agentdock-core` adds:
- `WorkspaceRootIdentity`
- `WorkspaceScope { version, roots }`

### Governed IDE capability policy

`agentdock-core` adds:
- `ActionGrant::{Denied, Allowed, ApprovalRequired}`
- `WorkspaceCapabilityPolicy`

The safe default allows read/search and requires approval for edit/test/debug/terminal. This deliberately does not model "whole IDE access" as unrestricted authority.

### Deterministic multi-root resolution

`project-resolver` adds `resolve_workspace`, which:
- rejects an empty workspace;
- rejects a zero root budget;
- rejects requested root counts over the configured budget;
- resolves every root with the existing AgentDock project resolver;
- builds a normalized stable root key;
- rejects duplicate root keys;
- sorts roots deterministically;
- returns versioned workspace scope.

Focused tests cover deterministic ordering, duplicate roots, empty/invalid limits, over-budget roots and safe capability defaults.

## Later phases

### Phase B — bounded multi-root index
- generated/vendor/ignore pruning;
- bounded file inventory;
- windowed text and symbol search;
- immutable source anchors;
- per-root index receipts and stale-index detection;
- no outside-root traversal.

### Phase C — isolated parallel execution
Reuse the Pragma donor model rather than inventing a second scheduler:
- WorkspaceTask;
- AgentRun;
- WorktreeBinding;
- AttentionRequest;
- one task -> N isolated worktrees -> N runs;
- no autonomous winner selection or merge.

### Phase D — governed IDE action gateway
Expose read/search/edit/test/debug/terminal actions only through capability checks, exact-root authorization, approval gates for gated actions and immutable action receipts.

### Phase E — operator workspace
Original UI over AgentDock facts:
- multi-root explorer;
- agent lanes and task/run state;
- attention queue;
- diffs/evidence;
- explicit wait/collision reasons.

### Phase F — platform and performance certification
Verify exact commits on supported OSes. Benchmark index latency/memory/completeness and worktree isolation/restart behavior before making performance or support claims.

## Clean-room boundaries

- No Codetist source was used.
- No Codetist branding, UI assets, wording or private APIs are copied.
- Public behavior is treated as evidence, not as a specification of private internals.
- No upstream/vendor parity claim.
- No unrestricted shell/network/IDE authority.
- No automatic push/merge/deploy.
- No production-readiness claim from unit tests.
- No platform claim until independently exercised.
- No performance/scale claim until measured.

## Phase A acceptance gates

- Rust formatting/check/tests on the existing matrix.
- Workspace contract serialization compiles across supported CI platforms.
- Deterministic root-order test passes.
- Duplicate and over-budget root tests pass.
- Capability default test proves mutation/execution is not silently allowed.
- Exact-head evidence is attached before Phase A is called verified.
