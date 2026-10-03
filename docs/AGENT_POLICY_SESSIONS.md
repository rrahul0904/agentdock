# Agent policy session visibility

AgentDock can inspect the bounded local session registry produced by Autonomous Forge AUTO-049 without gaining any additional mutation authority.

## Commands

```bash
cargo run -p agentdock-cli --bin agentdock-policy -- sessions --root /path/to/autonomous-forge
cargo run -p agentdock-cli --bin agentdock-policy -- receipts --root /path/to/autonomous-forge --limit 20
cargo run -p agentdock-cli --bin agentdock-policy -- receipts --root /path/to/autonomous-forge --limit 20 --json
```

## What is read

Only these Forge artifacts are read:

- `.ai/agent-sessions/<session-id>/registration.json`
- `.ai/agent-sessions/<session-id>/delivery-receipts/*.json`

The reader validates `agent-session-registration/v1` and `agent-policy-delivery-receipt/v1` exactly, including session/project identity, lowercase SHA-256 policy hashes, idempotency filenames, outcome values, bounded text, and registration/receipt agreement.

## What is deliberately not read

AgentDock does **not** read:

- session inbox request payloads;
- acknowledgement payloads;
- policy contents such as deployment mode, Docker/local-server flags, or any future secret-bearing data;
- Forge SQLite state.

The command is read-only and performs no writes, signals, Git operations, network calls, deployment actions, session fan-out, or policy delivery.

## Truth boundary

A policy is displayed as `acknowledged` only when a durable Forge `agent-policy-delivery-receipt/v1` says `outcome=acknowledged`. `refused` and `timeout` remain distinct outcomes. Missing registry state is reported as an empty inventory rather than fabricated session state.

## Safety bounds

- registry/session/receipt symlinks are refused;
- JSON files are capped at 64 KiB;
- session inventory is capped at 512;
- receipt inventory is capped at 500;
- identifiers use the same bounded safe syntax as Forge;
- receipt filename must equal its idempotency key;
- receipt project must match the registered session project.

This is an operator visibility layer only. Policy authoring/delivery remains owned by Autonomous Forge.
