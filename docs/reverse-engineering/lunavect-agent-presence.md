# Lunavect → AgentDock capability donor

## Status

Research/specification only. This document does **not** claim the Lunavect-derived capability is implemented or host-certified in AgentDock.

Tracker: RE-227  
Source discussion: https://www.reddit.com/r/coolgithubprojects/s/Ad5h6aQcSX  
Upstream: https://github.com/lovach/Lunavect  
Observed public release during research: 0.1.9  
Upstream license: MIT

## Product thesis

Lunavect demonstrates that local coding-agent work benefits from a compact presence surface that answers four questions without opening every terminal:

1. Which agent sessions exist?
2. Which sessions are actively working or waiting for me?
3. How much provider allowance remains and when does it reset?
4. Where has observed agent working time been spent?

AgentDock already owns the lower control-plane primitives: project/process discovery, coding-agent ancestry attribution, durable registry, local API/MCP, stable project routing and owner-scoped resources. The donor therefore maps into AgentDock as a **local agent presence and attention layer**, not as a separate application.

## Observed donor behavior

### Sessions

The public app unifies Claude Code and Codex sessions and exposes conservative states including working/thinking, permission required, input required, response ready, idle and unknown/stale. It supports search, provider/active filters, pin/hide/order controls, and bounded return-to-session behavior.

Important boundary: saved titles, catalog rows and old events are not treated as proof that work is currently active. Evidence ages out.

### Provider connection

The donor uses official local clients. Authentication remains with those clients. Provider setup can add lifecycle handlers/status bridges while preserving unrelated configuration and writing backups before modification.

Claude observations can come from lifecycle hooks and status/usage surfaces. Codex can use local app-server/runtime information with bounded local fallbacks.

### Quotas

Usage limits are observations with reset timestamps. Missing or expired values remain unavailable instead of being guessed. This is a useful contract for AgentDock: quota state must carry provenance/freshness, not just a number.

### Activity

Activity is working-time estimation, not token usage, CPU time or billing. Live intervals are counted only when work is freshly supported at both ends of a short observation window. Waiting/idle/unknown gaps do not become work.

Concurrent sessions from the same provider are unioned rather than double-counted; the combined total unions providers as well. Project totals similarly union overlapping sessions per project.

### Privacy

Session metadata and activity remain local. The donor records identity/state/timing fields but does not persist copies of conversation text, reasoning, tool arguments or tool output. Widgets read shared local snapshots rather than polling providers directly.

## AgentDock clean-room architecture

### 1. Domain contracts

```text
AgentSession
  id
  provider
  project_id?
  process_identity?
  client_kind?
  title?
  path?
  created_at?
  last_seen_at

PresenceEvidence
  session_id
  source_kind
  observed_at
  expires_at
  state
  tool_name?
  confidence = exact | supported | fallback

AttentionState
  session_id
  kind = permission | input | response_ready
  observed_at
  superseded_at?

QuotaObservation
  provider
  window_kind
  remaining?
  limit?
  reset_at?
  observed_at
  availability = available | unavailable | stale

ActivityInterval
  provider
  session_id?
  project_id?
  start_at
  end_at
  evidence_kind
  quality = observed | recovered
```

Unknown and stale are first-class states. No adapter may map missing evidence to idle, done, zero quota or success.

### 2. Evidence reducer

Provider observations feed a deterministic reducer:

```text
adapter observations
      ↓
identity normalization
      ↓
freshness + precedence policy
      ↓
SessionPresence projection
      ↓
attention events / activity intervals / notifications
```

Precedence should favor explicit fresh lifecycle/permission events over catalog metadata and file timestamps. Late events cannot resurrect an already superseded state.

### 3. Provider adapters

Use the AgentDock adapter boundary rather than embedding Claude/Codex rules in the UI.

Initial adapters:
- Claude Code: process attribution + supported lifecycle observation
- Codex: process attribution + supported local runtime observation

Future adapters can implement the same evidence contract for Cursor, Gemini CLI or other AgentDock-attributed runtimes.

Fallback readers must be bounded by time/entry/byte budgets and never interpret arbitrary file presence as liveness.

### 4. Attention engine

Emit an attention event only on deterministic transition into:
- permission required
- input required
- response ready

Notifications are projections of these events. A stale session cannot keep generating alerts.

### 5. Activity ledger

Observed work should be stored as short intervals and merged with interval-union semantics:
- per session
- per provider
- per project
- combined provider total

Recovered historical intervals, if added later, must be labeled separately from directly observed intervals.

### 6. Quota contract

Implement quota as a provider-neutral observation interface:

```text
readQuota(provider) -> QuotaObservation
```

The model requires observation time and reset time when known. Unavailable/stale is preserved. Quota collection must not require AgentDock to own provider credentials.

### 7. Operator surfaces

Phase A can ship without a native menu bar:
- local API
- CLI session/attention/activity views
- MCP read tools

Phase B:
- macOS menu-bar client
- WidgetKit snapshot adapter
- local notifications
- terminal/client jump-back

The native client remains a thin projection over AgentDock's durable local control plane.

## Minimal Phase A implementation

1. Add `AgentSession`, `PresenceEvidence`, `AttentionState`, `ActivityInterval` persistence.
2. Add a deterministic presence reducer with explicit freshness policy.
3. Feed Claude/Codex observations from AgentDock's existing attribution layer.
4. Expose:
   - list sessions
   - filter by provider/state/project
   - list attention-needed
   - activity totals by provider/project
5. Add pin/hide preference storage without affecting observation.
6. Emit local notification events but keep OS delivery behind an adapter.
7. Add fixtures and tests.

## Test matrix

- fresh working → working
- evidence expiry → unknown/stale
- permission event outranks catalog idle
- response-ready supersedes working
- late working event cannot resurrect completed turn
- hidden session still contributes to activity
- two concurrent same-provider sessions count once in provider aggregate
- Claude + Codex overlap counts once in combined aggregate
- missing quota remains unavailable
- no persisted record contains prompt/reasoning/tool arguments/tool output
- ambiguous navigation target fails closed

## Explicitly unclaimed

Until separately implemented and verified, AgentDock does not claim:
- Lunavect UI parity
- provider quota parity
- macOS WidgetKit integration
- native menu-bar delivery
- Terminal/iTerm exact-tab navigation
- keep-awake behavior
- recovery of historical activity
- signed/notarized macOS distribution

## Next concrete action

Implement only the Phase A persistence + reducer + read-only API/CLI slice, with Claude/Codex fixtures and exact-head CI. Defer native UI, quota scraping, notifications delivery and terminal automation until the core evidence model is deterministic and privacy-tested.
