# Contributing to keybr-tui

Thanks for considering a contribution to keybr-tui.

## Ways to Contribute

### Bug reports and feature requests

Open an issue with the [bug report](.github/ISSUE_TEMPLATE/bug_report.md) or
[feature request](.github/ISSUE_TEMPLATE/feature_request.md) template. The bug report asks for
the steps, your terminal and the version.

### Code contributions

1. Fork the repo and branch from `main`.
2. Keep a PR to one feature or fix, with tests for new behavior.
3. Run the [checks](#checks).
4. Open a PR using the template.

Contributions are licensed under AGPL-3.0-only.

## Development Setup

You need a stable Rust toolchain no older than `rust-version` in `Cargo.toml`, and a terminal
with ANSI colors. `cargo run` builds and starts the app; `cargo run -- --help` lists the flags.

The directory map is in [AGENTS.md](AGENTS.md#layout).

## Checks

CI runs every command below on each pull request. Run them before you push:

```sh
scripts/check-docs.sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

`cargo fmt --all` fixes what the format check reports. `just check` runs every check except the
release build.

## Code Style

Follow the rules in [AGENTS.md](AGENTS.md#rules). Keep the UI minimal and the dependencies few.

## Commit Messages

Start the summary with a type (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`) and
write it in the imperative mood. Details go after a blank line.
