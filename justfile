# keybr-tui developer task runner — recipes for common workflows. See https://just.systems

# Default: list all available recipes
default:
    @just --list

# Build the debug binary
build:
    cargo build

# Run the binary, forwarding any extra arguments (e.g. `just run --help`)
run *ARGS:
    cargo run -- {{ARGS}}

# Run the test suite
test:
    cargo test

# Format all sources in-place
fmt:
    cargo fmt --all

# Verify formatting without modifying files (used in CI)
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy, treating warnings as errors (used in CI)
lint:
    cargo clippy --all-targets -- -D warnings

# Pre-push gate: formatting, lints, and tests must all pass
check: fmt-check lint test

# Build the optimized release binary
release:
    cargo build --release

# Remove the target/ directory
clean:
    cargo clean

# Install the binary from the local source tree into ~/.cargo/bin
install:
    cargo install --path .

# Security-audit dependencies (requires `cargo install cargo-audit` first)
audit:
    cargo audit
