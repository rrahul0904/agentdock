# Laurel donor analysis: persistent cloud computers for coding agents

Status: clean-room research / architecture only  
Tracker: RE-232  
Canonical destination: AgentDock  
Source checked: 2026-09-24

## 1. Source and evidence boundary

Primary public sources:

- Live product: https://laurel.dev/
- Launch post: https://www.reddit.com/r/sideprojects/comments/1woyr9v/i_built_laurel_so_your_agents_dont_have_to_live/
- Creator description: https://www.reddit.com/r/BuildWithClaude/comments/1wodhlq/how_to_keep_claude_code_running_after_you_close/
- Advertised demo path: https://laurel.dev/workspace-preview
- Historical public repository: https://github.com/laureldev/laurel.dev

Important boundary:

The public repository currently describes an older Laurel product: a web-search/fetch API and MCP service. The live product launched on 2026-09-23/24 is instead an always-on cloud machine for Claude Code and Codex. Therefore:

- do not infer the current product's implementation from the old repository
- treat the old repository as historical product evidence only
- keep all implementation work clean-room
- use public behavior and creator statements as requirements, not proprietary source code

## 2. Observed product promise

The creator publicly describes the current Laurel as a service that:

1. creates a cloud machine for a coding agent
2. runs Claude Code and Codex on that machine
3. keeps the repository, work, context, and environment persistent
4. continues working after the user's laptop closes or loses Internet
5. lets the user reconnect from a browser on another device, including a phone
6. keeps using the user's own Claude Code/Codex account
7. provisions each machine as a Google Cloud VPS
8. encrypts user data

The central product insight is not merely "remote terminal". The durable value is:

- a persistent compute environment
- persistent repository/workspace state
- persistent coding-agent session identity and task continuity
- device-independent reconnection
- a simplified alternative to manually operating a VPS

## 3. Why Laurel belongs in AgentDock

AgentDock already owns the local and remote control-plane primitives that Laurel-like behavior needs.

Repository-certified or active AgentDock lineage already includes:

- coding-agent process/session attribution
- durable project/session registry
- lifecycle logs
- loopback control API
- owner-scoped resources
- device pairing
- short-lived one-time pairing challenges
- explicit local approve/deny
- Ed25519 paired-device possession proof
- durable revocation
- authenticated outbound WSS relay client
- replay-protected transport sessions
- read-only remote session inventory/logs
- parameter-bound, expiring, one-shot approvals
- fail-closed refusal to expose a generic shell or arbitrary process control

Laurel therefore should not become a second control plane.

It should extend AgentDock with a **persistent remote compute plane**.

Conceptually:

```text
Browser / phone
      |
      | authenticated AgentDock remote session
      v
Hosted AgentDock gateway / relay
      |
      | tenant + machine identity
      v
Persistent cloud machine
      |
      +--> AgentDock worker / daemon
      |
      +--> workspace + git repo
      |
      +--> Claude Code / Codex adapter
      |
      +--> durable task/log/session state
```

## 4. Product capability decomposition

### 4.1 Account and workspace

Expected user-facing objects:

- account
- workspace / organization
- member
- project
- repository connection
- machine
- machine image/profile
- coding-agent installation
- coding-agent session
- task
- task event
- approval
- secret reference
- device
- audit event
- usage/cost record

A minimal first version can be single-user, but the data model must remain tenant-scoped from day one.

### 4.2 Machine creation

Core user flow:

1. user selects "create machine"
2. selects region/profile
3. selects or connects repository
4. selects Claude Code or Codex
5. machine enters provisioning
6. cloud provider creates VM + encrypted persistent disk
7. bootstrap installs AgentDock worker/runtime
8. repo is cloned or workspace is restored
9. health/attestation arrives
10. machine becomes ready
11. user opens the coding-agent session

### 4.3 Long-running work

Required semantics:

- task initiation is durable before execution starts
- client disconnect must not cancel the task
- browser network loss must not terminate the agent
- daemon restart must recover task/session state
- VM restart must recover repository/workspace state
- reconnecting client must replay bounded event history
- duplicate client submissions must be idempotent
- session continuity must not depend on an open websocket

### 4.4 Reconnect from another device

Required semantics:

- paired/authenticated device
- explicit machine/session selection
- bounded event replay
- live follow mode after replay
- send a new prompt into a known coding-agent session
- approvals routed through AgentDock trust policy
- revocation takes effect immediately

### 4.5 Persistence

Separate persistence concerns:

1. **compute lifecycle state**
   - provider instance id
   - state transitions
   - operation idempotency keys

2. **workspace persistence**
   - repo checkout
   - uncommitted files
   - build caches if allowed
   - agent-local configuration

3. **AgentDock control state**
   - project/session identity
   - paired devices
   - approvals
   - audit records

4. **task/event persistence**
   - user prompts
   - status transitions
   - bounded stdout/stderr or normalized agent events
   - generated artifacts metadata

## 5. Clean-room target architecture

### 5.1 Control plane

Recommended control-plane responsibilities:

- identity
- tenant authorization
- machine desired state
- provider orchestration
- machine lease ownership
- quota/budget enforcement
- repository connection metadata
- secret references
- audit records
- hosted relay
- browser API

Do not place raw provider or user secrets in browser-visible records.

### 5.2 Compute provider interface

Create a provider-neutral abstraction before implementing GCP.

Pseudo-contract:

```rust
trait ComputeProvider {
    async fn create_machine(request: CreateMachineRequest) -> Result<ProviderMachine>;
    async fn get_machine(provider_id: &str) -> Result<ProviderMachine>;
    async fn start_machine(provider_id: &str, op_key: &str) -> Result<Operation>;
    async fn stop_machine(provider_id: &str, op_key: &str) -> Result<Operation>;
    async fn delete_machine(provider_id: &str, op_key: &str) -> Result<Operation>;
    async fn attach_volume(...);
    async fn snapshot_volume(...);
}
```

The interface must be designed for:

- idempotency
- eventual consistency
- provider retries
- partially completed operations
- unknown outcomes after network loss
- reconciliation from actual provider state

### 5.3 Machine lifecycle state machine

Canonical state model:

```text
requested
   |
   v
provisioning
   | \
   |  \--> failed
   v
ready
 |   \
 |    \--> suspending --> suspended --> resuming --> ready
 |
 +--> stopping --> stopped --> starting --> ready
 |
 +--> deleting --> deleted
```

Every transition should record:

- transition id
- machine id
- previous state
- new state
- reason
- actor
- desired state
- provider operation id
- idempotency key
- created_at

Invalid transitions must fail closed.

### 5.4 Machine agent

Each VM should run a narrow AgentDock machine worker.

Responsibilities:

- register machine identity
- prove workload identity
- report health
- report AgentDock daemon readiness
- report supported agent runtimes
- receive desired workspace/session commands
- emit normalized events
- reconnect outbound after network changes

The worker must not accept unauthenticated public control traffic.

### 5.5 Hosted gateway / relay

Prefer outbound machine connections.

Benefits:

- no public SSH required
- no need to bind AgentDock daemon publicly
- easier revocation
- centralized tenant authorization
- fewer inbound firewall rules

Gateway responsibilities:

- authenticate user/device
- authorize tenant/machine/session
- bind browser session to paired AgentDock identity
- route read/write envelopes
- preserve replay protection
- rate-limit
- never become the authority for local machine execution

Local machine AgentDock remains responsible for:

- local paired-device trust
- session ownership
- capability policy
- approval checks
- action parameter binding

### 5.6 Repository ingress

Phase A options:

- public git clone
- user-provided HTTPS token through secret reference
- GitHub App installation token later

Do not store raw git credentials inside repository config files.

Recommended target:

- GitHub App per workspace
- short-lived installation tokens
- machine fetches token through authenticated secret broker
- token expires quickly
- audit token issuance
- never write token to task logs

### 5.7 Secret management

Model secrets by reference.

Example:

```text
SecretRef
- id
- tenant_id
- type
- provider
- created_at
- rotated_at
- revoked_at
```

Actual values should live in:

- cloud secret manager
- KMS-wrapped encrypted storage
- or a dedicated secret broker

Rules:

- browser receives opaque secret ids only
- logs redact known credential patterns
- VM receives only secrets needed for that machine/session
- secret scope is tenant + machine/workspace
- revoke on machine deletion where appropriate

### 5.8 Storage

Use separate durable stores:

**Control-plane database**
- PostgreSQL
- machine desired/current state
- tenancy
- task metadata
- audit events
- approvals
- usage

**Persistent machine disk**
- git checkout
- user workspace
- agent files
- caches governed by policy

**Object storage**
- optional snapshots
- exported artifacts
- bounded log archive
- encrypted backups

### 5.9 Encryption

Public evidence only says data is encrypted; it does not specify the method.

Target clean-room design:

- TLS in transit
- cloud-provider disk encryption by default
- optional customer-managed KMS key later
- envelope encryption for sensitive control-plane fields
- encrypted object storage
- no plaintext secrets in logs

Do not claim parity with Laurel's encryption implementation without evidence.

## 6. Data model

### Workspace

```text
workspace
- id
- owner_user_id
- name
- created_at
- deleted_at
```

### Machine

```text
machine
- id
- workspace_id
- provider
- provider_instance_id
- region
- profile
- desired_state
- observed_state
- persistent_volume_id
- image_version
- created_at
- updated_at
- deleted_at
```

### Machine operation

```text
machine_operation
- id
- machine_id
- operation_type
- idempotency_key
- provider_operation_id
- status
- error_code
- error_detail_redacted
- created_at
- completed_at
```

### Project workspace

```text
project_workspace
- id
- machine_id
- repository_id
- checkout_path
- default_branch
- current_head
- created_at
```

### Agent runtime

```text
agent_runtime
- id
- machine_id
- kind            # claude_code | codex | ...
- runtime_version
- install_state
- auth_state      # configured | needs_user_auth | revoked
- created_at
```

### Agent session

Reuse/extend AgentDock's durable session concept.

Additional hosted metadata:

```text
hosted_agent_session
- id
- agentdock_session_id
- machine_id
- project_workspace_id
- runtime_id
- status
- started_at
- last_event_at
- ended_at
```

### Task

```text
task
- id
- session_id
- client_request_id
- prompt_ref
- status
- approval_state
- created_at
- started_at
- finished_at
```

### Task event

```text
task_event
- id
- task_id
- sequence
- event_type
- payload_redacted
- created_at
```

Unique key:

```text
(task_id, sequence)
```

### Audit event

```text
audit_event
- id
- workspace_id
- actor_type
- actor_id
- machine_id
- session_id
- action
- result
- parameter_hash
- created_at
```

## 7. API surface

### Machine API

```text
POST   /v1/machines
GET    /v1/machines
GET    /v1/machines/{id}
POST   /v1/machines/{id}:start
POST   /v1/machines/{id}:stop
POST   /v1/machines/{id}:suspend
POST   /v1/machines/{id}:resume
DELETE /v1/machines/{id}
```

Mutating requests require a client idempotency key.

### Workspace API

```text
POST /v1/machines/{id}/projects
GET  /v1/machines/{id}/projects
```

### Agent API

```text
POST /v1/projects/{id}/sessions
GET  /v1/projects/{id}/sessions
GET  /v1/sessions/{id}
POST /v1/sessions/{id}/tasks
GET  /v1/tasks/{id}/events
```

### Remote stream

Prefer an authenticated websocket/SSE hybrid:

- REST for durable commands
- SSE for replay/live read events
- websocket only where duplex low-latency input is needed

This keeps the durable task contract separate from a socket lifetime.

## 8. Phase A: smallest truthful implementation

The first implementation should **not** provision a real cloud VM.

Implement:

1. provider-neutral machine contracts
2. deterministic in-memory/fake compute provider
3. durable state machine
4. machine reconciliation loop
5. hosted machine/workspace records
6. link from hosted machine to AgentDock durable agent session
7. reconnect-safe task/event stream
8. owner/tenant isolation tests
9. idempotency tests
10. failure/retry tests

End-to-end test:

```text
create machine
-> fake provider transitions provisioning -> ready
-> attach project workspace
-> create agent session
-> create long-running fake task
-> append events
-> disconnect client
-> append more events
-> reconnect client
-> replay missing events
-> send follow-up prompt
-> stop machine
-> start machine
-> confirm workspace/session metadata survives
-> delete machine
-> confirm further actions fail closed
```

## 9. Phase B: GCP implementation

Public evidence says Laurel uses Google Cloud VPS instances.

A clean-room GCP adapter should consider:

- Compute Engine instance templates or direct instance creation
- persistent disk
- project/network isolation
- startup script or image-based bootstrap
- service account workload identity
- Secret Manager
- Cloud KMS
- Cloud Logging/Monitoring only for platform metadata; avoid raw user prompts by default
- budget/quotas
- region selection
- static egress policy if necessary

Security defaults:

- no public SSH
- no password auth
- outbound AgentDock worker tunnel
- minimal service-account IAM
- shielded VM where available
- hardened base image
- automatic patch strategy
- serial console disabled unless explicitly needed for break-glass operations

## 10. Phase C: real Claude Code/Codex workspace

Do not attempt to bypass provider authentication.

Target behavior:

- runtime is installed
- user is told when provider auth is required
- user completes supported auth flow
- raw account credentials are not copied into the web app
- runtime process is launched by a narrow adapter
- AgentDock attributes the process/session
- prompts target that session, not an arbitrary shell

Provider-neutral adapter:

```text
CodingAgentAdapter
- detect()
- install()
- auth_status()
- start_session(project)
- submit_input(session, input)
- stream_events(session)
- stop_session(session)
```

## 11. Browser/mobile UX

### Machine list

Each card:

- machine name
- region
- agent kind
- project
- state
- last activity
- current task state
- cost/budget indicator
- open / stop / delete

### Machine detail

Panels:

- agent conversation/task stream
- project/session identity
- machine health
- git status summary
- pending approvals
- audit history
- restart/stop controls

### Mobile

Prioritize:

- task status
- last events
- send next prompt
- approve/deny
- stop machine

Do not attempt to reproduce a full IDE on mobile in Phase A.

## 12. Security and abuse model

### Threats

- stolen browser session
- stolen pairing token
- session fixation
- replay
- cross-tenant machine enumeration
- workspace breakout
- secret exfiltration
- generic-shell escalation
- malicious repository bootstrap
- dependency-install scripts
- unbounded cloud cost
- crypto mining / abuse
- data remanence after deletion
- supply-chain compromise in machine image

### Required mitigations

- tenant-scoped authorization on every object lookup
- opaque ids
- short-lived user sessions
- AgentDock paired-device model
- replay protection
- parameter-bound approvals
- egress policy
- VM quota per account
- CPU/runtime budgets
- image signature/version provenance
- safe bootstrap allowlist
- secret redaction
- delete verification
- immutable audit log

### Fail-closed rules

- unknown machine owner -> deny
- unknown AgentDock session -> deny
- unsupported agent runtime -> deny
- missing approval for mutation -> deny
- expired device/session -> deny
- provider state ambiguous -> reconcile before repeating destructive operation
- cloud budget exhausted -> stop provisioning
- lost secret broker access -> do not substitute plaintext secrets

## 13. Cost model

Primary cost drivers:

- VM runtime
- persistent disk
- snapshots/object storage
- egress
- relay/gateway
- database
- observability

A viable service needs:

- per-machine size limits
- region/profile catalog
- idle policy
- optional auto-suspend
- monthly workspace budget
- hard daily spend ceiling
- provider operation reconciliation
- usage records tied to machine ids

Do not make "unlimited compute" claims.

## 14. Reliability model

### Desired-state reconciliation

The control plane stores desired state.

A reconciler compares:

```text
desired_state != observed_provider_state
```

and executes a bounded operation.

This avoids coupling correctness to a single request/response cycle.

### Idempotency

Every create/start/stop/delete request includes:

- client request id
- idempotency key
- expected machine version

Duplicate requests return the previous operation rather than creating duplicate VMs.

### Crash recovery

After process restart:

- reload pending machine operations
- query provider
- reconcile actual state
- resume only safe operations
- mark ambiguous failures for controlled reconciliation

## 15. Observability

Metrics:

- provisioning latency
- machine-ready success rate
- reconnect success
- task continuation success after client loss
- per-machine runtime minutes
- approval latency
- relay disconnect frequency
- agent crash/restart frequency
- VM reconciliation errors
- deletion completion latency
- budget enforcement events

Logs:

- structured
- redacted
- tenant/machine/session correlation ids
- no raw secrets
- no raw provider tokens

Traces:

- create machine request
- provider operation
- bootstrap callback
- AgentDock registration
- workspace ready
- agent session ready

## 16. Test strategy

### Unit

- state transition validation
- idempotency
- tenant authorization
- secret redaction
- approval binding
- quota/budget policy

### Integration

- fake provider reconciliation
- Postgres persistence
- AgentDock hosted-session mapping
- event replay
- client reconnect

### Security

- replayed remote envelope
- revoked device
- stolen old pairing challenge
- session fixation
- cross-tenant object id
- malformed provider callback
- malicious task parameter expansion
- log secret leakage

### Cloud contract

Against a dedicated test GCP project:

- create VM
- encrypted disk attached
- bootstrap AgentDock worker
- no public SSH
- outbound relay works
- stop/start preserves workspace
- delete removes instance
- disk/snapshot cleanup verified

### Browser E2E

Required hosted evidence before launch claim:

1. create machine
2. connect repo
3. start Claude Code or Codex session
4. submit long task
5. close browser
6. continue task server-side
7. reopen on another device
8. replay missed events
9. send next prompt
10. exercise an approval
11. stop/restart machine
12. verify workspace persists
13. delete machine
14. verify access is gone

## 17. Delivery plan

### Slice 1 — contracts

- machine model
- machine operation model
- compute provider trait
- fake provider
- state machine
- tests

### Slice 2 — hosted AgentDock binding

- machine worker identity
- hosted machine registration
- session mapping
- durable task/event stream
- reconnect test

### Slice 3 — GCP test adapter

- dedicated test project
- VM lifecycle
- encrypted disk
- startup/bootstrap
- outbound worker registration
- stop/start persistence

### Slice 4 — browser continuation

- machine list
- machine detail
- session events
- prompt submission
- approvals

### Slice 5 — production hardening

- KMS/secret broker
- quotas/budgets
- image hardening
- delete verification
- incident/audit exports
- rate limits
- abuse controls

## 18. Explicit non-goals for the first slice

- generic web terminal
- public SSH endpoint
- arbitrary shell execution from browser
- full browser IDE
- multi-cloud parity
- unattended provider-login automation
- unlimited VM/runtime promise
- copying Laurel branding or proprietary UI/assets
- claiming current Laurel code equivalence from the historical public repository

## 19. Competitive/product differentiation opportunity

The clean-room AgentDock version can go beyond the observed Laurel promise by making the trust boundary explicit:

- same AgentDock session identity locally and remotely
- paired-device cryptographic identity
- parameter-bound approvals
- no public shell
- auditable machine/session lifecycle
- provider-neutral compute contract
- local machine + cloud machine under one control plane
- portable project/session identity

This turns the Laurel capability from "host my coding agent on a VPS" into "governed persistent execution for coding agents across local and cloud machines."

## 20. Production-certification boundary

The repository can truthfully claim only what is independently proven.

Repository evidence can prove:

- schemas
- state machines
- provider interfaces
- fake-provider behavior
- authorization tests
- replay/idempotency tests
- AgentDock integration contracts

It cannot prove:

- target GCP credentials
- real VM creation
- encrypted-volume configuration
- cloud networking
- hosted relay
- browser-to-cloud-machine end-to-end flow
- exact deployed commit
- real cost controls

Those remain external certification gates until live evidence exists.
