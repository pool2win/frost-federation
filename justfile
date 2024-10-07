default: test

test:
	RUST_LOG=debug cargo test

build:
	cargo build

run config="config.toml":
	RUST_LOG=debug cargo run -- --config-file={{config}}

check:
	cargo check
