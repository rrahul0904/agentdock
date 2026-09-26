# RE-258 — VibeOnGo Cloud Workspace Donor

Status: **clean-room reverse engineering / implementation pending**  
Canonical destination: **AgentDock**  
Issue: #undefined  
Research date: 2026-09-25

## 1. Evidence boundary

Primary sources reviewed:

- Reddit launch thread: https://www.reddit.com/r/SaaS/s/7Q2HAqjRJX
- First-party product site: https://vibeongo.com/
- First-party app/login surface: https://app.vibeongo.com/
- Public implementation reference: https://github.com/jashandeep31/vibeongo
- Public source head reviewed: `e8197cf72e4155524569e86e7f936cd21cb0489a`

The public VibeOnGo repository is licensed under Elastic License 2.0. This document is a behavioral and architectural study. AgentDock must implement its own contracts, code, UX and tests; do not copy VibeOnGo source text, proprietary assets, trade dress, or undisclosed implementation details.

## 2. Product definition

VibeOnGo is best understood as a **remote software-development control plane**.

The product loop is:

1. Connect a GitHub repository or create a repository.
2. Save a reusable project/environment blueprint.
3. Launch an isolated VM or sandbox.
4. Bootstrap source, tools, scripts, services, secrets and agent configuration.
5. Attach one or more coding agents.
6. Keep shell/terminal work reconnectable.
7. Expose selected service ports through HTTPS preview routes.
8. Control the session from web or mobile.
9. Trigger isolated work from GitHub events such as pull requests or issues.
10. Meter runtime and terminate idle or completed compute.

The product is therefore not “an editor in the cloud.” The durable product object is the project blueprint plus session history/control metadata; compute can remain replaceable.

## 3. Reddit launch thread — feedback audit

The visible public thread contains one top-level criticism and two author replies.

### Criticism

A commenter said the product looked like a T3 copy.

### Author clarification

The author distinguishes VibeOnGo by saying:
- the coding environment runs in a remote sandbox,
- the workspace can be controlled from mobile or web,
- the user's laptop does not need to remain on,
- a preconfigured project session can start with one click,
- the coding surface is familiar because it uses the OpenCode SDK,
- T3 can connect directly into a VibeOnGo sandbox session.

### Product requirement derived from the feedback

Do not lead with a chat/editor replica. Lead with:
- **workspace lifecycle**
- **remote compute**
- **resume/reconnect**
- **preview routing**
- **automation**
- **mobile control**

T3/OpenCode/Codex/Pi belong behind agent adapters. AgentDock should not imitate another product's visual identity.

## 4. Public implementation anatomy

The public repository provides unusually strong evidence for the component split.

### Runtime / data plane

The `core` directory is a Go runtime and proxy layer. Public paths expose concepts for:
- PTY/terminal sessions
- WebSocket transport and auth
- filesystem operations
- runtime stats
- tool provisioning
- scripts/services
- repository setup
- secrets/keys
- session/task management
- proxy routing
- MCP server/tools
- OpenCode inventory/session integration

### Control plane

The `platform` directory is a TypeScript pnpm/Turborepo workspace with:
- Next.js web app
- Expo mobile app
- API/server
- shared API hooks/client
- database package
- shared UI package
- documentation app

The server contains provider orchestration, sandbox setup jobs, onboarding, GitHub integration/webhooks, cron processing, WebSocket support, services and controllers.

### Provider abstraction

The public provider factory separates **VM** and **sandbox** runtimes.

Observed public provider adapters include:
- VM: AWS and DigitalOcean
- Sandbox: E2B, Daytona and Vercel Sandbox

For AgentDock this is evidence that provider-neutral lifecycle boundaries are useful; it is not a requirement to duplicate the same provider set.

### Durable domain objects visible in public schemas

Public schema names demonstrate separate models for:
- projects
- git repositories
- environments
- instances and instance metadata
- sandbox metadata
- project sessions
- project chats
- project automations
- project domain routing
- proxy domains
- job queues
- SSH keys
- auth sessions
- user wallet/runtime accounting

This supports a control-plane design where project identity, workspace sessions, compute instances, routes and automation runs are distinct records.

## 5. Clean-room AgentDock architecture

### 5.1 Control plane

AgentDock should own:
- identity and authorization
- project blueprints
- workspace-session lifecycle
- compute-provider dispatch
- bootstrap plans
- preview route registry
- automation admission
- usage records
- audit/provenance events

The control plane must never assume that a remote worker is authoritative for ownership, billing or policy.

### 5.2 Workspace agent

A small remote runtime should run inside provisioned compute and expose a narrow authenticated protocol for:
- health/capability inventory
- process/service start/stop
- PTY session create/attach/resize/input
- filesystem read/write/list under an allowed workspace root
- repo status/diff primitives
- runtime resource stats
- declared preview ports
- agent adapter lifecycle

It should not receive long-lived platform master credentials.

### 5.3 Provider interface

Use a provider-neutral contract such as:

- `CreateWorkspace(spec, idempotency_key)`
- `GetWorkspace(provider_id)`
- `TerminateWorkspace(provider_id, reason)`
- `ReconcileWorkspace(provider_id)`
- `GetUsage(provider_id, window)`

Provider-specific launch details stay behind adapters.

### 5.4 Preview gateway

Preview routing should be independent of compute provisioning.

Required properties:
- route keyed to project/session/service/port
- owner authorization before route registration
- short-lived worker registration credentials
- WebSocket support
- host-header and origin policy
- explicit public/private exposure state
- revocation on session termination
- no arbitrary host/port SSRF bridge

Stable project aliases may map to the latest authorized live session, but the route registry remains authoritative.

### 5.5 Terminal/session semantics

“Persistent terminal” should mean **reconnectable while the workspace/session exists**, not an unsupported promise that an ephemeral VM filesystem survives forever.

Model:
- terminal session id
- workspace session id
- command/shell metadata
- created/last-attached timestamps
- bounded event replay
- live PTY attachment state

A tmux-compatible adapter is optional; AgentDock's contract should not hard-code tmux as the only implementation.

### 5.6 Agent adapters

Define a neutral `CodingAgentAdapter` with capability negotiation.

Potential adapters:
- OpenCode
- Codex
- Pi
- T3-compatible connection/deep-link bridge
- future CLI agents

Keep agent transcripts and workspace control events separate from provider lifecycle.

## 6. Core state machines

### ProjectBlueprint

`draft -> ready -> archived`

Contains:
- repository reference
- default branch/base ref
- setup commands
- start commands/services
- tool requirements
- secret references
- network/preview declarations
- agent preferences
- idle policy

Blueprints contain **secret references**, not plaintext secret material.

### WorkspaceSession

`requested -> provisioning -> bootstrapping -> ready -> busy -> stopping -> stopped`

Failure substates must remain observable and retryable without duplicating infrastructure.

### ProviderInstance

`creating -> running -> terminating -> terminated`

The provider record is an implementation resource, not the user-facing session identity.

### AutomationRun

`received -> admitted -> queued -> workspace_requested -> executing -> evidence_ready -> awaiting_publish -> completed|failed|cancelled`

GitHub webhook receipt alone must not authorize code execution.

## 7. GitHub automation path

Recommended flow:

1. Verify webhook signature.
2. Normalize event to an immutable receipt.
3. Resolve repository installation + owner policy.
4. Evaluate automation rule.
5. Deduplicate on delivery/event identity.
6. Build an execution request with least-privilege token scope.
7. Launch isolated workspace.
8. Run agent with bounded budget/time/tool policy.
9. Capture patch/test/evidence artifacts.
10. Require policy/approval before publishing a review, branch or PR when configured.
11. Revoke credentials and terminate compute.

Issue text, PR bodies and repository content are untrusted input and must not directly become privileged shell commands.

## 8. Security and tenant isolation

Phase A must be fail-closed.

Required controls:
- owner id on every project/session/route/automation record
- unguessable session ids plus authenticated possession proof
- short-lived worker tokens
- encrypted secret storage with per-run materialization
- deny cross-project route attachment
- allowed workspace root for filesystem access
- command execution policy and audit receipt
- egress policy hooks
- max runtime and budget ceilings
- webhook signature verification and replay protection
- cleanup reconciliation for orphaned compute/routes/secrets

## 9. Metering and shutdown

Keep **usage measurement** separate from **billing**.

Record:
- session start/stop
- provider instance id/type/region
- measured runtime
- CPU/memory/network when supported
- provider-reported billable usage
- AgentDock-calculated usage
- termination reason
- idle-policy decision receipt

An idle policy should examine declared activity signals and create an auditable shutdown decision. Do not terminate solely because a web browser disconnected.

## 10. Mobile and web surfaces

Both clients should consume the same authenticated control-plane APIs.

Minimum mobile/web parity:
- project list
- workspace launch/stop/status
- coding-agent conversation
- diff/change inspection
- terminal attach
- preview links
- GitHub automation/run status
- approval actions

Avoid implementing a separate mobile control plane.

## 11. Reliability requirements

- create/terminate must be idempotent
- bootstrap must be restartable by phase
- worker reconnect must not create a second session
- route registration must be reconciled after worker loss
- event replay must be bounded and ordered
- shutdown must revoke routes/tokens even if provider deletion is delayed
- cron/reconciler must find orphaned provider instances
- provider outage must not corrupt durable project state

## 12. UX implications from the T3 comparison

The first-run experience should visually prove the remote-workspace value:

- show provider/region/session state
- show “laptop can disconnect” semantics
- show running services/previews
- show reconnectable terminals
- show mobile handoff
- make external agent integrations explicit

Do not optimize the main marketing surface around looking like a familiar AI chat.

## 13. Phase A — bounded implementation slice

Do **not** begin with AWS/E2B/Daytona/Vercel production APIs.

Implement:

1. `ProjectBlueprint`, `WorkspaceSession`, `ProviderInstance`, `PreviewRoute`, `AutomationRun` contracts.
2. In-memory or local durable repository for those entities.
3. Fake `ComputeProvider` supporting create/get/terminate/reconcile and injected failures.
4. Deterministic bootstrap-plan compiler.
5. Reconnect-safe task/terminal event stream using the existing AgentDock transport boundary.
6. Preview-route authorization registry with no real public tunnel yet.
7. Idle/metering reducer and termination decision receipts.
8. GitHub automation admission/dedup model using fixtures only.
9. Owner-isolation tests across every read/write/mutation.
10. Integration test: blueprint -> fake workspace -> bootstrap -> terminal event -> preview declaration -> idle stop -> cleanup.

## 14. Phase B

After Phase A exact-head tests:
- one real compute provider behind the same interface
- remote worker bootstrap
- authenticated PTY
- private preview gateway
- one agent adapter
- one GitHub event automation
- browser/mobile verification

Prefer one complete provider path over five partial adapters.

## 15. Phase C

Only after the first hosted path is certified:
- additional providers
- stable project preview aliases
- mobile push/approvals
- richer diff/review UX
- usage wallet/pricing
- Forgejo or alternate Git providers
- T3 handoff
- advanced MCP/tool inventory

## 16. Acceptance gates

Research completion is not implementation completion.

Do not raise parity/readiness status until evidence proves:
- exact-head CI green
- provider failure/reconcile tests green
- owner isolation tests green
- reconnect test green
- route ownership/revocation test green
- webhook replay/dedup test green
- idle-stop evidence green
- one hosted end-to-end workspace lifecycle green
- secrets absent from logs/evidence packs

## 17. Relationship to existing AgentDock donor work

### Laurel

Laurel contributes the persistent cloud-computer / durable remote coding-agent workspace concept. VibeOnGo broadens this into multiple VM/sandbox providers, project blueprints, preview routing, mobile/web control and GitHub automations.

### OpenTerm.app

OpenTerm contributes local ADE/workspace ergonomics. VibeOnGo contributes the remote execution/control plane. Keep local workspace UX separate from cloud provider lifecycle.

### AgentPort / existing AgentDock remote control

Existing authenticated transport/pairing/session primitives should be reused rather than creating a second remote-control protocol.

## 18. Explicit non-goals

- no source-code cloning
- no VibeOnGo branding/trade-dress copy
- no claim of provider parity
- no real cloud provisioning before fake-provider lifecycle tests
- no direct execution of raw webhook/user text
- no global secrets on remote workers
- no billing claims before reconciled usage evidence
- no “persistent” claim that exceeds actual lifecycle guarantees

## 19. Tracker status

Recommended tracker mapping:

- Tracker ID: RE-258
- Source: VibeOnGo
- Canonical product: AgentDock
- Relationship: capability donor / cloud sandbox workspace orchestration
- Completion: research/spec phase only
- Hosted status: source live; AgentDock donor slice not launch-certified
- Next action: implement Phase A contracts + fake provider + isolation/reconnect/route/idle tests
