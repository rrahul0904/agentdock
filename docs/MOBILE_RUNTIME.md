# AgentDock Mobile Runtime

Status: Phase A contract implementation on `reverse/pokecode-mobile-runtime`.

Tracker: RE-375 / issue #37.

## Why this exists

The mobile runtime line turns a phone into one governed execution target within AgentDock rather than creating a second autonomous-agent kernel. A session may eventually run locally on Android, attach to a desktop runtime, or attach to a remote/cloud runtime while preserving the same owner, project, workspace, session, evidence, approval, and recovery identities.

The first public behavior donor for this slice is Pokecode, supplied through its Reddit launch and public distribution/documentation repository. Pokecode's application source and signing material are described publicly as private/proprietary, so this repository uses only observable behavior and public documentation as product requirements. No Pokecode source, assets, branding, screenshots, private protocols, or UI trade dress are copied here.

## Product boundary

AgentDock owns the operator/runtime surface. Autonomous Forge remains authoritative for bounded implementation work, exact-SHA verification, policy decisions, durable evidence, and protected push/merge/deploy boundaries.

The differentiating target is not merely "Codex on Android." The target is one evidence-backed runtime model across:

- local Android execution;
- local desktop execution;
- remote/cloud execution;
- provider-neutral coding-agent sessions;
- explicit voice/task ingress;
- durable reconnect/recovery semantics.

## Phase A contracts

The `mobile-runtime` crate defines versioned, provider-neutral contracts only. It does not start processes, access a phone, call a model, perform Git writes, or expose a remote shell.

### `MobileRuntimeProfile`

Records platform, architecture, Android API level when applicable, runtime kind, workspace identity, isolation declaration, and capability assessments.

Local Android runtime kinds currently include `native-android` and `termux-proot`. A Termux/PRoot userland is explicitly not treated as strong process isolation. The contract refuses a local Android/PRoot profile that claims `remote-sandbox` isolation.

### `RuntimeHealthAssessment`

Every runtime capability is independently assessed as:

- `supported`
- `degraded`
- `unavailable`
- `unknown`

Absent evidence remains `unknown`. Platform names do not imply capability support.

Current capability vocabulary covers filesystem access, Git, terminal processes, agent app-server integration, voice ingress, background execution, preview serving, and Android builds.

### `MobileSessionBinding`

Binds an owner, project, workspace, runtime, and session to local or remote execution. The state reducer fails closed around invalid transitions:

`starting -> ready -> working -> needs-attention / reconnecting -> stopped | failed`

Additional valid recovery transitions are encoded explicitly; stopped and failed sessions are terminal in Phase A.

### `VoiceIngressEnvelope`

Voice is modeled as typed session input, not shell dictation. A new transcript is `submit=false` by default and must match the target owner/session before handoff. An explicit submit action is required before a future adapter may convert it into an agent turn.

## Phase A invariants

1. PRoot userland must never be reported as strong remote-sandbox isolation.
2. Missing runtime evidence must remain `unknown` rather than becoming optimistic support.
3. Owner/project/workspace/runtime/session identities must be non-empty and bounded.
4. Invalid session-state transitions must be refused.
5. Voice input must not request execution by default.
6. Voice input for one owner/session must not bind to another.
7. Local and remote execution modes must serialize deterministically.

The crate includes deterministic tests for these invariants.

## Next slices

### Phase B — runtime probe and Android adapter

Add a bounded, opt-in capability probe for filesystem, Git, PTY/process behavior, agent app-server viability, background execution, and preview ports. Emit a versioned runtime receipt with tool/platform versions and per-capability results. No Android compatibility claim should be made from host-only tests.

### Phase C — agent protocol bridge

Prefer a structured app-server/protocol adapter over terminal-screen scraping. Start with Codex where supported while retaining a provider-neutral AgentDock session interface. Credentials and raw secret-bearing protocol payloads must not enter generic event logs.

### Phase D — phone-native client

Build an original Android operator shell for projects, sessions, task/chat, voice input, terminal, approvals, attention state, previews, and evidence receipts.

### Phase E — project and Git lifecycle

Add explicit import/bind/sync and branch/worktree identity. Push, merge, release, and deploy remain behind existing AgentDock/Autonomous Forge governance and exact-revision evidence.

### Phase F — device/recovery certification

Exercise screen-off continuation, process death, application restart, reconnect, network loss, low-memory conditions, battery/thermal behavior, and an explicit Android device/API matrix.

### Phase G — cross-runtime continuation

Prove that one governed task/session can continue across phone, desktop, and remote runtimes while preserving identity and evidence cursors. Do not claim seamless migration until runtime evidence demonstrates it.

## Truthful status rules

Unit tests can establish contract correctness; they cannot establish Android compatibility. Terms such as `working on Android`, `background capable`, `voice ready`, `production ready`, or `seamless continuation` require exact-revision device/runtime evidence. A missing or failed probe is represented as unknown/degraded/unavailable rather than converted into a success claim.
