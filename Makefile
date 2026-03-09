.PHONY: build install test lint

build:
	cargo build

install:
	cargo install --path .

lint:
	cargo check
	cargo clippy -- -D warnings

test: lint
	cargo test
