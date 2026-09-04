.PHONY: check test scan fmt

check:
	cargo check --workspace

test:
	cargo test --workspace

scan:
	cargo run -p agentdock-cli -- scan

fmt:
	cargo fmt --all
