# Styx capability donor: governed multi-agent control plane

Tracking issue: #39 (`RE-376`)

## Boundary

This is a clean-room capability integration into AgentDock, Autonomous Forge, and Agent Control Plane. It is not a Styx clone and does not reuse Styx branding, logo, screenshots, trade dress, private data, or undocumented implementation details.

Primary public donor evidence:

- Reddit launch: `r/coolgithubprojects` post `1wzrgul`
- Public repository: `NicholasFlemmer/styx-app`
- Public product/docs: `heystyx.com`
- Upstream code license: Apache-2.0; upstream states that the Styx name/logo are not included in that license.

## Observed donor behavior

Public evidence shows a local desktop control plane for existing coding-agent CLIs. Useful behaviors include:

- project and agent visibility in one operator surface;
- task-local Git worktrees/branches;
- change review, undo/revert, land/publish flows;
- local application preview and UI-targeted follow-up feedback;
- MCP plus provider CLI wrappers for access requests;
- short-lived provider grants and local secure-secret storage;
- stronger human-presence requirements for production mutation;
- access request/use/revoke audit history.

The upstream project explicitly describes itself as a guardrail rather than a sandbox. AgentDock must preserve that distinction: local-user execution, container isolation, and remote sandbox execution are separate capability levels.

## Public upstream gaps that become requirements

At intake time the upstream issue tracker documents several useful failure modes:

1. normal Git credentials can allow `git push`/`git fetch` outside the provider grant path;
2. a long-lived grant on a provider that cannot narrow credentials can cover more than one later production mutation;
3. coverage thresholds are not fully enforced in CI;
4. stale UI wording can imply behavior the runtime does not actually provide;
5. fine-grained edit tracking can be disabled, weakening hunk-level review.

These are upstream reports, not locally reproduced defects.

## Portfolio mapping

Do not create another standalone desktop agent runtime.

- **AgentDock** owns local project/session/operator surfaces, preview routing, agent attribution, attention, remote clients, and control-plane projection.
- **Autonomous Forge** owns autonomous task discovery/selection, isolated execution, validation, independent verification, recovery, scheduling, and portfolio advancement.
- **Agent Control Plane** owns delegated authority, deterministic policy, approvals, credential/tool governance, budgets, audit, and rollback.

## Extension thesis

The valuable extension is an evidence-governed autonomous development control plane, not a tiled terminal UI.

Compared with a human-operated local shell, the combined product should add:

- autonomous tracker -> plan -> delegate -> execute -> verify -> advance;
- restart-safe leases, heartbeats, recovery, and evidence;
- cross-project dependencies and capacity-aware scheduling;
- exact-SHA merge/deploy impact gates;
- a local credential edge broker with parameter/target/environment/time/use-bound grants;
- prevention of silent ambient-credential fallback inside governed sessions;
- authenticated web/mobile supervision over the same durable runtime;
- reconstructable evidence lineage from task intent through production action.

## Phase A1: AgentDock governed attention contracts

This branch introduces provider-neutral projection types only. It does not issue credentials, execute provider commands, merge, push, deploy, or change process authority.

### CapabilityRequest

Carries bounded identity and context:

- project/task/session identity;
- provider and optional target;
- environment classification;
- action class;
- canonical SHA-256 intent digest;
- requested TTL and use count;
- human-readable reason.

Conservative candidate rule:

- only `observe` / `read` in known non-production environments can be considered for policy auto-approval;
- `unknown`, production, remote mutation, deploy, database write, and delete are never auto-approval candidates in this projection.

The policy engine remains authoritative.

### CapabilityGrantView

Read-only projection of grant state. It intentionally contains no token, key, credential, secret, environment-variable value, or credential-handle field.

A grant is active only when:

- decision is `approved`;
- remaining uses is greater than zero;
- an expiry exists and is still in the future.

Denied, expired, revoked, consumed, zero-use, and missing-expiry states fail closed.

### AttentionItem

Unifies durable operator attention categories:

- needs input;
- review;
- access request;
- verification failed;
- merge gate;
- deploy gate.

Items have bounded identifiers, a source evidence reference, and deterministic timestamp/id ordering.

## Phase A1 acceptance checks

- unknown environment or action never becomes an auto-approval candidate;
- production never becomes an auto-approval candidate, including reads;
- remote writes and deploys never become auto-approval candidates;
- intent digest must be exact lowercase SHA-256 hex;
- expired, revoked, or consumed grants cannot render active;
- attention ordering is deterministic;
- no raw credential field exists in the public grant projection;
- repository-wide CI must pass before merging.

## Deferred authority

Not implemented by this slice:

- OS keychain access;
- biometric/physical-presence checks;
- provider credential issuance;
- Unix socket / named-pipe broker;
- CLI wrappers;
- Git credential helper;
- remote approval transport;
- Forge action-risk projection;
- push/merge/deploy mutation.

Those remain later independently reviewable phases behind explicit evidence and authority boundaries.
