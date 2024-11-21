# Set default log level if not provided through environment
export LOG_LEVEL := env_var_or_default("RUST_LOG", "info")

default: test

test:
	RUST_LOG={{LOG_LEVEL}} cargo test

build:
	cargo build

# For log level use RUST_LOG=<<level>> just run
run config="config.toml":
	RUST_LOG={{LOG_LEVEL}} cargo run -- --config-file={{config}}

check:
	cargo check
