# Contributing to keybr-tui

Thanks for your interest in contributing! This document covers the basics you need to get started.

## Development Setup

**Requirements:**
- Rust stable toolchain (MSRV: 1.75)
- A terminal emulator with ANSI color support

**Build and run:**

```sh
cargo build
cargo run
```

**Run tests:**

```sh
cargo test
```

**Lint and format:**

```sh
cargo fmt --all
cargo clippy -- -D warnings
```

## Pull Request Process

1. Fork the repository and create a feature branch from `main`.
2. Keep PRs focused: one feature or fix per PR.
3. Include tests for new functionality where applicable.
4. Ensure `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` all pass before submitting.
5. Write a clear PR description explaining the what and why of your changes.

## Architecture

The project follows a Model-View-Update (MVU) pattern:

- `src/app.rs` -- Application state (the Model). All mutable state lives here.
- `src/update.rs` -- Event handling and state mutations (the Update). Never renders.
- `src/ui.rs` -- Rendering (the View). Reads state, never writes it.
- `src/engine/` -- Text generation algorithm (Markov chains, letter scheduling).
- `src/metrics.rs` -- Per-key statistics and confidence tracking.
- `src/persistence.rs` -- Saving/loading stats and config.
- `src/events.rs` -- Terminal event channel (keyboard input, tick events).

Project layout:

```text
keybr-tui/
├── src/
│   ├── main.rs           # Entry point
│   ├── app.rs            # Central state (MVU)
│   ├── update.rs         # State transitions
│   ├── ui.rs             # Rendering (read-only state)
│   ├── events.rs         # Input + tick event channel
│   ├── tui.rs            # Terminal setup/teardown
│   ├── metrics.rs        # Per-key statistics
│   ├── config.rs         # Config file parsing
│   ├── persistence.rs    # Stats save/load
│   ├── engine/           # Adaptive text generation
│   └── components/       # UI widgets
├── docs/                 # User-facing docs (comparison, FAQ, troubleshooting)
└── .github/workflows/    # CI and release
```

For detailed architecture documentation, see the `docs/` directory.

## Code Style

- Run `cargo fmt` before committing. CI enforces formatting.
- All clippy warnings are treated as errors in CI.
- Terminal colors must use ANSI values only (no hex/RGB). See `docs/brand_identity.md`.
- Keep the UI minimalist. When in doubt, leave it out.

## Reporting Issues

- Use the GitHub issue templates for bug reports and feature requests.
- For bugs, include your OS, terminal emulator, and terminal size if relevant.
