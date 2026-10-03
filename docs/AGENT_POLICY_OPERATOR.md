# Agent Policy Session / Receipt Operator View

`agentdock-policy` is a read-only companion to the AgentDock Supervisor Console for the registered local agent-session contract implemented by Autonomous Forge AUTO-049.

It does **not** send policies, discover arbitrary chats, read prompt/conversation content, or claim a session received anything without a durable Forge delivery receipt.

## Registered sessions

```bash
agentdock-policy sessions --root /path/to/autonomous-forge
agentdock-policy sessions --root /path/to/autonomous-forge --json
```

The command reads only direct registered-session metadata under:

```text
<forge-root>/.ai/agent-sessions/<session-id>/registration.json
```

It validates `agent-session-registration/v1` and shows only:

- session ID;
- project ID;
- registration timestamp.

## Policy delivery receipts

```bash
agentdock-policy receipts --root /path/to/autonomous-forge
agentdock-policy receipts --root /path/to/autonomous-forge --limit 50
agentdock-policy receipts --root /path/to/autonomous-forge --json
```

The command reads only durable outcomes under:

```text
<forge-root>/.ai/agent-sessions/<session-id>/delivery-receipts/<idempotency-key>.json
```

It validates `agent-policy-delivery-receipt/v1` and shows only:

- session ID;
- project ID;
- idempotency key;
- policy SHA-256;
- outcome (`acknowledged`, `refused`, or `timeout`);
- bounded refusal/timeout reason when present;
- completion timestamp.

The runtime-policy request file and its policy body are intentionally **not read or rendered** by this tool.

## Safety / truth boundary

- Registry/session/receipt symlinks are refused.
- Traversal is direct and non-recursive.
- Session count is capped at 512.
- Receipt inventory/limit is capped at 500.
- Each JSON artifact is capped at 64 KiB.
- Session directory name must match registration `session_id`.
- Receipt project must match the registered project.
- Receipt filename must match the idempotency key.
- Policy hashes must be 64 lowercase hexadecimal characters.
- A refusal must include a bounded reason.
- A missing `.ai/agent-sessions` registry is reported as an empty result rather than an error.

This is observation only. It adds no direct Forge SQLite access, request write, session fan-out, process mutation, shell, Git, or deployment authority.
