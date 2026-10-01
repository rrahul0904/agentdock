# RE-354 — Context-efficiency outer harness

## Purpose

This branch starts the independent AgentDock implementation inspired by the public intentic project and the supplied Reddit discussion. It does **not** claim vendor parity or reproduce intentic branding/UI.

The Phase A goal is intentionally narrow: give coding-agent sessions a deterministic, bounded view of workspace topology and runtime facts, while emitting evidence receipts that can later support controlled paired benchmarking.

## Public research sources

- Reddit: https://www.reddit.com/r/coolgithubprojects/comments/1wuie0w/youre_probably_wasting_50_of_your_tokens_i_can/
- Product/docs: https://intentic.dev/
- Public source: https://github.com/intentic/intentic
- AgentDock tracker: https://github.com/rrahul0904/agentdock/issues/20

The upstream repository is MIT-licensed. Our implementation is independently structured for AgentDock and does not copy upstream UI assets, marketing copy, screenshots, fixtures or benchmark results.

## Verified donor concepts

The public project documents an outer harness around installed coding agents, with isolated worktrees, project/environment context, ranked code search, durable usage accounting, automations/approvals, and paired benchmark tooling. The Reddit thread also contains direct skepticism about headline token-savings claims and asks for controlled evidence.

That feedback becomes an acceptance requirement here: **quality and correctness must pass before a token/cost reduction is reported**.

## Phase A implemented in this branch

`agentdock-context` provides:

- metadata-only deterministic workspace mapping;
- conservative default exclusion of VCS internals, dependency/build caches, symlinks and common credential/key filenames;
- versioned resource facts with source, confidence, priority and a sensitive bit;
- byte-bounded context-capsule rendering;
- deterministic non-cryptographic experiment arm assignment;
- normalized efficiency/outcome receipts where unavailable metrics stay `null`, never silent zero;
- focused unit tests for determinism, exclusions, budget, priority, unknown metrics and receipt validation.

### Security/privacy boundary

Phase A does not read source-file contents. Sensitive facts are not rendered by the default capsule. The current filename filters are intentionally conservative; later phases should add explicit repository policy configuration rather than silently weakening them.

### Measurement boundary

A receipt is evidence plumbing, not evidence of savings. The fields support provider/model/harness, token/cache counts, cost, duration, tool calls, first-write timing, context size/hash and an accepted/rejected/unknown outcome. No field is fabricated when the provider does not expose it.

## End-to-end phase plan

### Phase B — retrieval

Add a persistent AgentDock-owned lexical/symbol index with ranked path/line anchors, outline/read/refs/impact operations and explicit weak/no-answer semantics. Semantic retrieval remains optional and must have model/version provenance.

### Phase C — runtime composition

Join the context capsule to the exact AgentDock project, worktree and coding-agent session. Feed existing registry/service/port facts through the typed `ResourceFact` boundary. Refresh/invalidate only on observed topology/runtime changes. Persist a digest of each injected bundle.

### Phase D — paired benchmark

Integrate receipts with Token Intelligence rather than creating a second billing system. Use pinned commits, byte-identical tasks, isolated caches, randomized arm order and repeated runs. Compare cost/tokens/turns/latency only among runs that pass the same outcome/test/security gates.

### Phase E — operator surface

Expose capsule provenance, omitted-data reasons, usage/quality comparisons, opt-out controls and review surfaces. Do not publish savings percentages until the paired benchmark is reproducible across a documented task/model/provider population.

## Non-claims

This Phase A slice does not establish:
- a particular token-savings percentage;
- intentic feature parity;
- semantic-search quality;
- provider billing accuracy beyond values actually supplied to the receipt;
- cross-platform runtime integration;
- production readiness.
