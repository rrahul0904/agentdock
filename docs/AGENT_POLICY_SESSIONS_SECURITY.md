# Security boundary for agent policy session visibility

The viewer is intentionally a passive reader over the Autonomous Forge AUTO-049 file registry.

It may read only registration metadata and durable delivery receipts. It does not traverse session inboxes or acknowledgement directories, and it never evaluates or displays runtime-policy contents.

Path controls:
- Forge root must resolve to a directory.
- `.ai` and `.ai/agent-sessions` symlinks are refused.
- session directories and delivery receipt files may not be symlinks.
- registry resolution must stay under the selected Forge root.

Data controls:
- JSON artifacts are capped at 64 KiB.
- identifiers are syntax-bounded.
- policy hashes must be lowercase 64-character SHA-256 strings.
- receipt outcomes are limited to `acknowledged`, `refused`, and `timeout`.
- project/session and filename/idempotency relationships are revalidated.

The viewer does not write Forge state, signal processes, call the network, invoke Git, deploy software, or contact agent sessions.
