# CLAUDE.md — keybr-tui Agent Context

This file is the entry point for any AI agent working on this codebase. It lists the available reference documents, what each one covers, and when you should read it.

**Rule**: Do not guess. If you are uncertain about intent, architecture, or visual design — read the relevant doc before writing code.

---

## Reference Documents

### [`docs/project_overview.md`](docs/project_overview.md)
**Read when**: Starting a new session, unsure of the project's scope, or deciding whether a feature belongs in the MVP.

Covers: Core objective, MVP scope, tech stack (Rust + ratatui + crossterm), the MVU architecture pattern, and the text generation engine's origin (Keybr algorithm from `aradzie/keybr.com`).

---

### [`docs/architecture.md`](docs/architecture.md)
**Read when**: Working on the event loop, metrics tracking, text rendering, cursor positioning, or the text generation algorithm.

Covers: The 5 critical implementation concerns — algorithm translation from TypeScript, per-key metric tracking, the async tick/input event loop, coordinate geometry for cursor rendering on resize, and the text span styling model.

---

### [`docs/brand_identity.md`](docs/brand_identity.md)
**Read when**: Adding or modifying any UI widget, choosing colors, or deciding on layout density.

Covers: The "Purist" minimalist aesthetic, the terminal-native ANSI color palette (exact ratatui `Style` values), and design rules for the active typing session screen.

---

### [`docs/ratatui.md`](docs/ratatui.md)
**Read when**: Working on anything in `ui.rs`, adding widgets, debugging rendering, handling terminal resize, or setting up the event loop.

Covers: Immediate-mode rendering model, terminal setup/teardown, layout constraints, key widgets (`Paragraph`, `Block`), span construction pattern for the typing display, the tick event loop pattern, and common pitfalls.

---

### [`docs/rust_reference.md`](docs/rust_reference.md)
**Read when**: Hitting a borrow checker issue, implementing the event channel, timing keystrokes, or choosing a collection type for state.

Covers: Ownership and borrowing rules, structs/impl, enums + pattern matching, traits, closures, `mpsc` channels for the event loop, `std::time::Instant` for reaction timing, and error handling with `anyhow`.

---

### [`docs/file_structure.md`](docs/file_structure.md)
**Read when**: Creating a new file, deciding where code belongs, or understanding the responsibility boundary between modules.

Covers: Full `src/` directory layout, the responsibility of each module (`main.rs`, `app.rs`, `update.rs`, `ui.rs`, `events.rs`, `engine/`, `metrics.rs`), and naming/organization conventions.

---

## Project Constraints (Always Apply)

- **MVP only**: Do not implement menus, profile persistence, config files, or multiplayer. Defer everything to post-MVP.
- **Algorithm fidelity**: The text generator must mirror Keybr's algorithm exactly — no wordlists, no random characters.
- **State in `App` only**: No mutable globals. All runtime state lives in `src/app.rs`.
- **Strict MVU separation**: `ui.rs` reads state, never writes it. `update.rs` writes state, never renders.
- **Terminal-native colors only**: No hex/RGB. Use ANSI colors as defined in `docs/brand_identity.md`.

## Environment

- OS: Arch Linux, shell: fish (use `;` not `&&`; use `set -x` not `export`)
- Build: `cargo build`, run: `cargo run`
- Editor: nvim
