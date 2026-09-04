# Testing Strategy

## Current automated checks

GitHub Actions runs the Rust workspace on:

- Ubuntu
- macOS
- Windows

Checks:

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test --workspace`

## Unit tests

### agentdock-core
- stable hostname slug behavior
- default service visibility

### framework-detection
- framework fingerprint classification

### project-resolver
- fallback project identity

### port-manager
- reservation lifecycle

### process-discovery
- representative macOS `lsof` parser fixture
- representative Linux `lsof` parser fixture
- IPv6 endpoint parsing

## Phase 2 additions

The daemon phase should add:

- SQLite migration tests
- reconciliation state-machine tests
- repeated-scan idempotency
- stale/orphan transition tests
- local API contract tests
- restart persistence tests

## Manual smoke test

```bash
cargo run -p agentdock-cli -- doctor
cargo run -p agentdock-cli -- scan
cargo run -p agentdock-cli -- scan --all
cargo run -p agentdock-cli -- scan --json --all
```
