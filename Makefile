.PHONY: check test scan scan-all fmt

check:
	cargo check --workspace

test:
	cargo test --workspace

scan:
	cargo run -p agentdock-cli -- scan

scan-all:
	cargo run -p agentdock-cli -- scan --all

fmt:
	cargo fmt --all
