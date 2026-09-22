# System One bounded-decision donor → AgentDock

Status: research / clean-room implementation contract  
Tracker: RE-217  
Source: https://www.reddit.com/r/coolgithubprojects/s/S8vDTQxwlL  
Upstream: https://github.com/iamaamir/system-one  
Canonical destination: `rrahul0904/agentdock`

## Why this belongs in AgentDock

The public project is not a full agent runtime. Its useful idea is a small, provider-neutral
**bounded decision primitive** that agents and routers can consume without binding application
code to a specific classifier/model.

AgentDock already owns local agent identity, projects, sessions, policy boundaries, MCP exposure,
and durable control-plane state. The donor therefore maps to AgentDock as a decision/evaluation
capability, not as a new standalone product or a second orchestration framework.

## Publicly observable contract

The public System One material exposes three bounded question families evaluated against shared
state:

- **choice** — select one option from a known answer set and return probabilities/confidence.
- **noul** — evaluate a binary proposition as a probability.
- **score** — place the state on an ordered rubric and return a score plus distribution metadata.

The public runtime also demonstrates these architectural ideas:

1. Consumers depend on a stable provider interface rather than a vendor-specific client.
2. HTTP providers can share one `/v1/systemone`-style transport contract.
3. Multiple heterogeneous questions can be batched against the same state.
4. Responses are validated fail-closed before application logic sees them.
5. Provider metadata (latency/request ID/token counts) is kept separate from application decisions.
6. Abort/timeout and response-size boundaries exist at the transport edge.
7. Routing/escalation policy stays outside the decision primitive.
8. Agent-facing tool integration is a consumer of the primitive, not part of the core abstraction.
9. Credentials are configuration, not persisted decision state.

## Important source/license boundary

The upstream GitHub repository currently reports **no repository license** through the GitHub API.
Treat it as a behavioral/architectural reference only.

Do not copy upstream source code, tests, wording-heavy implementation material, package names,
branding, or proprietary fixtures into AgentDock unless the upstream author later publishes terms
that explicitly permit reuse.

This branch should contain an original implementation designed from public behavior and interfaces.

## AgentDock target contract

Introduce a small provider-neutral decision surface in `agentdock-core`.

Suggested original domain model:

```text
DecisionRequest
  shared_state
  questions[]
    ChoiceQuestion
    PropositionQuestion
    OrderedScoreQuestion
  optional provider/model hint

DecisionProvider
  evaluate(request, deadline/cancellation) -> DecisionResponse

DecisionResponse
  answers keyed by question id
  provider metadata
  request provenance
```

### Required invariants

- At least one question is required.
- Question IDs are unique.
- Choice answers may only select declared choices.
- Probability-like values must be finite and within [0, 1].
- Every requested question must have exactly one compatible answer.
- Score legends/distributions must match the declared ordered rubric.
- Unknown answer types, missing answers, malformed distributions, NaN/inf values, and oversized
  responses fail closed.
- Timeouts/cancellation must be distinguishable from provider/protocol failures.
- No automatic retry of a potentially side-effectful higher-level agent action.
- Provider output must not directly mutate sessions, execute commands, approve actions, or select a
  remote-control capability.
- AgentDock policy remains authoritative after a decision is returned.

## Intended AgentDock uses

This primitive can later support bounded decisions such as:

- classify a task before choosing a workflow,
- select an allowed tool family,
- decide whether a human approval gate is required,
- score risk/complexity,
- choose an agent handoff target,
- choose retrieval/no-retrieval,
- choose a model tier inside a separate routing policy,
- decide whether to escalate to a stronger model/provider.

The decision layer must never bypass the existing approval, pairing, ownership, or session
boundaries.

## Phase plan

### Phase A — deterministic contract + mock provider

Repository-only implementation.

1. Add versioned decision request/question/answer types to `agentdock-core`.
2. Add a `DecisionProvider` trait with a deterministic in-memory/mock implementation.
3. Add fail-closed validation for:
   - missing/duplicate answers,
   - wrong answer family,
   - unknown choice,
   - invalid probabilities,
   - malformed score rubric/distribution,
   - empty request.
4. Add provider metadata/provenance fields without mixing them into the decision answer.
5. Add tests proving malformed provider output never reaches policy consumers.
6. Add one bounded consumer example that maps a synthetic request to a workflow class without
   executing anything.

### Phase B — HTTP provider boundary

1. Add an opt-in HTTP provider.
2. Explicit connect/read timeout and max-response-size limits.
3. Cancellation propagation.
4. Bounded endpoint allow-list/configuration; no arbitrary URL from untrusted tool arguments.
5. Secrets from environment/secure config only; never persisted in session logs.
6. Request IDs/latency/usage captured as evidence.
7. No retry loop by default.

### Phase C — AgentDock MCP/read surfaces

1. Read-only provider status/capabilities.
2. Optional bounded `evaluate_decision` tool only after policy review.
3. Redact secrets and sensitive shared state in logs.
4. Persist audit metadata only when the caller explicitly opts into history.

### Phase D — policy/routing consumers

Use the primitive from separate policy modules for tool/workflow/model routing.

The decision provider proposes a bounded result; AgentDock policy validates permissions and remains
the final authority.

### Phase E — external-provider certification

Only after repository implementation is green:

- authorized test endpoint,
- timeout/cancellation integration tests,
- malformed live-response tests,
- credential redaction evidence,
- exact-head CI evidence,
- no claim of vendor parity without contract fixtures and explicit source terms.

## Smallest truthful next implementation slice

Implement **Phase A only**:

- original Rust request/question/answer types,
- `DecisionProvider` trait,
- deterministic mock provider,
- strict response validator,
- unit tests for fail-closed behavior,
- one non-executing classification example.

Do not wire a live Jev/Reflex endpoint, model router, automatic tool execution, approval bypass,
generic HTTP target, or hosted claim in this first slice.

## Repository readiness claim

This document starts the reverse-engineering track only. It does **not** claim that AgentDock
already implements System One-compatible runtime behavior or that any live provider has been
certified.
