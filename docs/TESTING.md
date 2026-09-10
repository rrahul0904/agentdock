# Testing Strategy

## CI matrix

The repository CI configuration runs Rust checks on Ubuntu, macOS, and Windows:

- cargo fmt --all -- --check
- cargo check --workspace
- cargo test --workspace

## Phase 1 tests

- stable hostname slug
- discovery fixture parsing
- framework fingerprints
- project fallback identity
- port reservation lifecycle
- Windows listener/parser fixture

## Phase 2 tests

Registry:
- service ID survives project port changes
- active -> stale -> orphaned transitions
- orphaned -> active resume

Daemon:
- event query parsing
- boolean query flags

## Manual smoke test

Terminal 1:

~~~bash
cargo run -p agentdockd
~~~

Terminal 2:

~~~bash
cargo run -p agentdock-cli -- doctor
cargo run -p agentdock-cli -- daemon status
cargo run -p agentdock-cli -- daemon services --all
cargo run -p agentdock-cli -- daemon projects
cargo run -p agentdock-cli -- daemon events
~~~

Restart the daemon and confirm IDs persist in ~/.agentdock/agentdock.db.
