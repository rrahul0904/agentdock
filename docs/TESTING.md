# Testing Strategy

## CI matrix

Rust:

- Ubuntu
- macOS
- Windows

Checks:

- cargo fmt --all -- --check
- cargo check --workspace
- cargo test --workspace

MCP:

- Node 22
- pnpm install
- TypeScript build of @agentdock/mcp-server

## Current unit coverage

Discovery:

- representative macOS/Linux listeners
- Windows listener parsing
- IPv6 endpoint parsing

Core:

- hostname slug
- lifecycle-state round trip

Frameworks:

- representative runtime fingerprints

Agent attribution:

- Codex
- Claude Code
- Cursor
- regular Node negative case

Port manager:

- reservation ownership
- wrong-owner release rejection

Registry:

- stable service ID across port changes
- active/stale/orphaned lifecycle
- route follows port changes
- route collision suffixes

Proxy:

- Host extraction
- hostname normalization

Daemon:

- bounded HTTP header parsing
- event query parsing
- wildcard service target mapping

## Manual smoke test

Terminal 1:

~~~bash
cargo run -p agentdockd
~~~

Terminal 2:

~~~bash
cargo run -p agentdock-cli -- daemon status
cargo run -p agentdock-cli -- daemon services --all
cargo run -p agentdock-cli -- daemon projects
cargo run -p agentdock-cli -- daemon routes
~~~

MCP:

~~~bash
pnpm install
pnpm mcp:build
pnpm mcp:dev
~~~

Use the MCP Inspector or a compatible host to call the registered tools.
