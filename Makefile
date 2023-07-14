.PHONY: all
all: build lint test

.PHONY: build
build:
	cargo build --features test-support

.PHONY: lint
lint:
	cargo fmt --all -- --check
	cargo clippy --features test-support -- --no-deps --deny warnings -D clippy::unwrap_used
	cargo clippy --tests -- --no-deps --deny warnings -A clippy::unwrap_used
