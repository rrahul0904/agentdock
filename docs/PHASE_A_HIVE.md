# Issue #18 — Phase A: bounded local hive slice

Source contract: [AgentDock issue #18](https://github.com/rrahul0904/agentdock/issues/18).
Research donor: Munder Difflin's HIVE.md and docs/ARCHITECTURE.md; this is a clean-room
implementation of only the local durable transport concepts, not a port of the application.

## Scope

- Version **1** strict serde contracts: Floor, Agent, Session, Task, Message,
  MemoryRef and Event. Unknown fields and unknown stored schema revisions fail closed.
- `agentdock-hive` shares the existing daemon's `agentdock.db` with namespaced `hive_*`
  tables. The daemon opens the hive on boot and reclaims **expired** leases; no separate
  Electron or Git-based event store is created.
- SQLite IMMEDIATE transactions and a UNIQUE (floor, recipient, sender, key) constraint
  implement atomic per-recipient inbox entries and idempotent enqueue. Reusing a key with
  different content is an error, not silent data replacement.
- Claim leases have unguessable tokens, bounded duration, compare-and-swap ack, explicit
  retry with three-attempt dead letter, and manual replay **only before ack**. Event cursors
  and task state survive restart. Reply enqueue and ack commit in the same transaction.
- Floor workspace is canonical and unique. An internal Scope must exactly match an active
  persisted session, its agent, its floor, owner and workspace. Destinations must belong
  to the same floor. Memory references must resolve to existing in-workspace files.
  Event reads are scoped to the participating agent; bodies are not written to events.
- Adapter trait + deterministic fake + **opt-in** Codex `exec` CLI subprocess:
  argument array (no shell), prompt on stdin, read-only sandbox, cwd pinned to scope.
  Typed turn events are written to SQLite. This is not native PTY integration or
  a provider hook. No raw model output, usage, cost, credentials or approvals are exposed.

## Recovery and semantics

Daemon bootstrap invokes `recover(now_ms())` on the same persisted database. An
unexpired lease is not stolen; after expiry it is retried with the **same message ID**
or dead-lettered at the attempt cap. Reopening the DB does not falsely claim that a
provider's native interactive session has resumed. `resume_session` rebinds only the
hive's logical ownership after an explicit interruption.

Enqueue and ack/reply are transactionally idempotent. A killed worker can still have
performed external CLI actions before its ack was persisted: those effects may repeat.
Thus external CLI execution is **at-least-once**, not an exactly-once guarantee.
Consumers with irreversible actions must provide their own idempotency or wait for
separate Phase B authorization. No arbitrary shell or process-kill API is exposed.

`run_once` is a synchronous **internal** bridge to be driven by a separate worker,
not invoked on the discovery/event loop. There is no public unauthenticated hive HTTP
endpoint, PTY attach/resume, built-in CLI supervision/timeout, or installed-Codex acceptance
claim. The Codex adapter defaults to read-only/no approval and suppresses output.

## Acceptance tests

`cargo test -p agentdock-hive` covers the three-agent floor, replies, task final
state and event cursor replay; duplicated key and conflicting content; lease expiry,
retry, dead-letter, manual unacked replay, prevention of acked replay; bounded-hop
loop detection; floor/session/workspace and memory isolation; and a separate test
process killed after claim/during a fake turn, followed by database restart/recovery.

CI's existing Rust matrix runs the workspace tests on Linux, macOS and Windows.
This does **not** validate real native PTYs or installed Codex across platforms.

## Out of scope

Phase B: destructive/spend/scope approvals, governance, identity/authenticated external
APIs, budgets, usage parity, log rotation and policy enforcement.

Phase C: operator console, task board UI, terminals, visual floor, intervention.

Phase D: voice, connectors and optional third-party integrations.

No upstream feature parity, release completeness or production readiness is claimed.
