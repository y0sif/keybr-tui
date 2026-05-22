# CLAUDE.md — keybr-tui Agent Context

Project constraints and environment for AI agents working on this codebase.

## Project Constraints

- **MVP only**: Do not implement menus, profile persistence, config files, or multiplayer. Defer everything to post-MVP.
- **Algorithm fidelity**: The text generator must mirror Keybr's algorithm exactly — no wordlists, no random characters.
- **State in `App` only**: No mutable globals. All runtime state lives in `src/app.rs`.
- **Strict MVU separation**: `ui.rs` reads state, never writes it. `update.rs` writes state, never renders.
- **Terminal-native colors only**: No hex/RGB. Use ANSI colors (the palette is defined in the engine module).

## Architecture

The project follows a Model-View-Update (MVU) pattern:

- `src/app.rs` — Application state (the Model). All mutable state lives here.
- `src/update.rs` — Event handling and state mutations (the Update). Never renders.
- `src/ui.rs` — Rendering (the View). Reads state, never writes it.
- `src/engine/` — Adaptive text generation (Markov chains, letter scheduling).
- `src/metrics.rs` — Per-key statistics and confidence tracking.
- `src/persistence.rs` — Saving/loading stats and config.
- `src/events.rs` — Terminal event channel (keyboard input, tick events).

## Environment

- OS: Arch Linux, shell: fish (use `;` not `&&`; use `set -x` not `export`)
- Build: `cargo build`, run: `cargo run`
- Test: `cargo test`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Editor: nvim

## Before pushing

Run all CI checks locally first — pushing with failing CI generates noise:

```fish
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```
