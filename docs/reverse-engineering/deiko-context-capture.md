# Deiko clean-room reverse-engineering map

Status: research + architecture slice started 2026-09-22

Canonical destination: `rrahul0904/agentdock`

Relationship: capability donor, not a standalone clone. The reusable capability is a local-first screen/voice/context-capture layer that produces an agent-ready brief and hands it to coding agents already discoverable by AgentDock.

## Public product behavior

Source product: https://deiko.app/
Source discussion: https://www.reddit.com/r/saasbuild/comments/1wnax1g/redesigned_my_ai_made_website_myself_to_human_one/
Additional implementation description: https://dev.to/maddy30445r/i-am-tired-of-taking-screenshots-and-explaining-them-to-claude-code-while-working-4bio

Observed public flow:

1. Global hotkey begins a capture session.
2. Cursor movement is recorded while the user speaks.
3. The user can hover/point at UI elements and lasso a region.
4. Relevant screen regions are captured and labelled.
5. Speech is transcribed; cursor timing and visual events are associated with the transcript.
6. OCR enriches the captured regions with visible text.
7. The session is reduced to a compact brief plus screenshots/paths.
8. A small floating handoff surface lets the user send/drop the context into an AI coding agent or any text-capable target.
9. Privacy is local-first: screenshots/briefs stay on-device; audio is transient; offline transcription is available.
10. A free tier continues locally after an initial cloud-transcription allowance; paid value is mostly faster cloud transcription/history/replay.

Public beta notes observed on 2026-09-22:

- macOS 14+ and Apple Silicon are the current supported platform on the product site.
- The site advertises 30 free cloud-transcription minutes followed by unlimited on-device transcription.
- Bring-your-own Groq key is offered during beta.
- Pro is advertised at $2.99/month or $24.99/year, with cloud transcription and session history.
- No account is required; licensing is presented as a local license key.

## Why this belongs in AgentDock

AgentDock already owns the local coding-agent control plane: process discovery, coding-agent ancestry attribution, project/service identity, stable local routing, loopback control API, durable registry, and MCP bridge. Deiko's strongest reusable capability is not another agent runtime; it is a **multimodal context-ingress surface** for the agents AgentDock already understands.

Therefore the clean-room product direction is:

`screen + pointer + voice + OCR events -> local context bundle -> AgentDock session/project attribution -> bounded handoff to selected agent`

Do not create a separate product unless the capture client later proves independently useful outside AgentDock.

## Clean-room architecture

### 1. Capture client

Create a desktop capture client with a narrow local permission boundary.

Responsibilities:

- global hotkey/session lifecycle
- active display/window metadata
- cursor sampling during an active session only
- explicit region/lasso capture
- screenshot crop generation
- local session state and temporary assets
- floating review/handoff UI

Platform strategy:

- Phase 1: macOS 14+ using ScreenCaptureKit + accessibility/event-tap APIs through a dedicated platform adapter.
- Phase 2: Windows capture adapter.
- Keep the session/event schema platform-neutral so AgentDock does not become macOS-specific.

### 2. Event timeline

Represent each capture session as ordered events instead of a pile of screenshots.

Suggested schema:

```text
CaptureSession
  id
  started_at / ended_at
  project_id?
  agent_session_id?
  displays[]
  transcript_segments[]
  events[]
  assets[]
  brief

CaptureEvent
  t_ms
  type: pointer | region | screenshot | app_focus | transcript_anchor
  display_id
  window_id?
  x / y
  region?
  asset_id?
  transcript_segment_id?
```

This lets later summarization answer *what the user was referring to when they said a phrase*.

### 3. OCR + visual enrichment

For each explicit or high-confidence region:

- crop locally
- OCR locally where possible
- record application/window title only when permissioned
- attach OCR text and geometry to the asset
- avoid continuous full-screen retention

The brief generator should reference only the most relevant screenshots/regions, not every sampled pointer location.

### 4. Speech providers

Provider-neutral transcription boundary:

- on-device transcription provider
- BYO cloud provider (initial candidate: Groq Whisper-compatible API)
- optional managed cloud transcription later

Contract:

```text
transcribe(audio_stream, locale_hint?) -> timestamped segments
```

Audio is ephemeral by default and deleted after successful transcript persistence. Failure paths must not silently retain raw audio.

### 5. Brief compiler

Input:

- timestamped transcript
- pointer/region timeline
- OCR text
- selected screenshots
- AgentDock project/session metadata

Output:

- concise problem statement
- explicit requested change/outcome
- ordered observations
- referenced screenshot paths/assets
- app/window context
- relevant project/service URL when AgentDock can resolve it
- provenance for each observation

The compiler must be deterministic enough that a user can preview/edit before handoff.

### 6. AgentDock integration

New bounded interfaces should be added rather than arbitrary shell injection.

Candidate daemon/API surface:

- `begin_context_capture`
- `attach_capture_to_project`
- `list_capture_sessions`
- `get_capture_bundle`
- `prepare_agent_handoff`

Handoff targets should reuse AgentDock's coding-agent attribution where possible:

- Claude Code
- Codex
- Cursor
- Gemini CLI
- generic clipboard/text target

Do not bypass the existing owner/session boundaries. A capture bundle attached to one AgentDock session must not be injected into another session without an explicit user action.

## UX to recreate, not copy

Core interaction pattern to preserve:

- one global start/stop gesture
- point while speaking
- optional explicit lasso
- short review surface
- one-action handoff

AgentDock-specific improvements:

- show the resolved project/service/branch before handoff
- let the user choose the exact detected agent session
- include stable `.localhost` preview URL automatically
- attach logs/health evidence only when explicitly selected
- show an evidence/provenance list before the agent receives the bundle

## Privacy/security contract

Fail closed on permissions and data movement.

Required defaults:

- capture only during an explicit active session
- screenshots and OCR local-only
- raw audio transient and deleted after transcription
- no arbitrary background screen recording
- no automatic upload of screen content to AgentDock servers
- secrets/redaction pass before optional cloud transcription/LLM summarization
- explicit indicator while capture is active
- TTL cleanup for temporary assets
- per-session ownership tokens inherited from AgentDock
- audit event for every handoff target

## Monetization model to borrow conceptually

Do not copy branding/pricing verbatim. The public model is useful because the expensive capability is cloud transcription, while local capture remains useful for free.

Possible AgentDock packaging:

- Free/local: capture, OCR, on-device transcription, brief generation, clipboard/manual handoff
- Pro: managed high-quality cloud transcription, session history/replay, multi-device sync later
- BYO provider key should remain available so core capability is not paywalled behind provider margin

## End-to-end implementation phases

### Phase A — repository-certified context bundle core

- platform-neutral capture event schema
- local bundle persistence
- deterministic brief compiler over synthetic fixtures
- privacy lifecycle tests
- AgentDock project/session binding contract

### Phase B — macOS capture client

- explicit global hotkey
- active-session cursor sampling
- lasso/crop screenshots
- local OCR
- floating review surface
- permission/error UX

### Phase C — voice pipeline

- timestamped on-device transcription
- BYO cloud adapter
- mixed-language preservation tests
- audio deletion guarantees

### Phase D — agent handoff

- clipboard/generic text target first
- Claude Code/Codex/Cursor/Gemini CLI targeting through AgentDock session attribution
- asset-path references and selected screenshots
- explicit confirmation before cross-session handoff

### Phase E — history + replay

- searchable local sessions
- replay timeline
- retention controls
- export/import bundle contract

### Phase F — release certification

- macOS permission/UAT matrix
- multi-monitor UAT
- privacy/redaction tests
- failure/restart tests
- CPU/memory/storage benchmark
- signed/notarized app distribution before production claims

## Smallest truthful implementation slice

Implement **Phase A only** first inside AgentDock:

1. Add the platform-neutral `CaptureSession/CaptureEvent/Asset` model.
2. Persist a local capture bundle under AgentDock ownership.
3. Compile a brief from a synthetic timestamped transcript + pointer/region fixture.
4. Bind the bundle to an existing AgentDock project/session.
5. Add tests proving no bundle can be read through a different owner token.

This creates the durable contract needed for the native macOS client without pretending screen/voice capture already works.

## Acceptance criteria for the first slice

- deterministic fixture produces the same brief and asset references across runs
- project/session ownership is explicit and enforced
- no new generic shell/process execution surface is introduced
- capture bundle format is documented and versioned
- unit tests cover malformed events, missing assets, ownership mismatch, and TTL cleanup
- README/status language says `capture contract implemented`, not `Deiko parity` or `desktop capture complete`

## Non-goals for the first slice

- production macOS recorder
- Windows support
- managed billing/licensing
- hosted cloud ingestion of screenshots
- automatic UI clicking or autonomous computer control

This is a clean-room behavioral reconstruction based on public product behavior and public implementation descriptions; no Deiko source code is required or assumed.