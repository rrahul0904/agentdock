# Agent policy session viewer verification

The `agentdock-policy` binary is covered by deterministic unit tests inside `crates/agentdock-cli/src/bin/agentdock-policy.rs` and is executed by the focused Supervisor Console workflow on Ubuntu, macOS, and Windows.

Required gates:

```bash
cargo fmt -p agentdock-supervisor -p agentdock-cli -- --check
cargo test -p agentdock-supervisor
cargo test -p agentdock-cli --bin agentdock-policy
cargo check -p agentdock-cli
```

Repository-wide CI must also remain green before merge.

The test suite covers missing-registry behavior, valid registration and acknowledged receipt rendering, project/session mismatch refusal, filename/idempotency mismatch refusal, refused-without-reason refusal, and symlinked-session refusal on Unix. The reader never opens request payloads, so a passing viewer test does not imply policy delivery or acknowledgement; those claims remain owned by Forge durable receipts.
