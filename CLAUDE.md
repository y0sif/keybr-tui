# CLAUDE.md — keybr-tui Agent Context

Project constraints and environment for AI agents working on this codebase.

## Project Constraints

- **MVP only**: Do not implement menus, profile persistence, config files, or multiplayer. Defer everything to post-MVP.
- **Algorithm fidelity**: The text generator must mirror Keybr's algorithm exactly — no wordlists, no random characters.
- **State in `App` only**: No mutable globals. All runtime state lives in `src/app/`.
- **Strict MVU separation**: `ui.rs` reads state, never writes it. `update/` writes state, never renders.
- **Terminal-native colors only**: No hex/RGB. Use ANSI colors (colors are set in `src/components/` and `src/ui.rs`).

## Architecture

The project follows a Model-View-Update (MVU) pattern:

- `src/app/`: Application state (the Model). All mutable state lives here.
  `mod.rs` defines `App` and its constructors. `focus.rs`, `lesson.rs` and
  `profile.rs` hold the rest of its methods.
- `src/update/`: Event handling and state mutations (the Update). Never renders.
  One file per screen, plus `agent.rs` for taria agent input (unix only).
- `src/ui.rs`: Rendering (the View). Reads state, never writes it.
- `src/screens.rs`: Screen structure shared by `update/` and the views (menu
  items, settings rows, progress table geometry). Depends on no `App` state.
- `src/components/`: The widgets and screens `ui.rs` draws.
- `src/tree.rs`: The taria accessibility tree, a second read-only view for
  agents. Unix only.
- `src/engine/`: Adaptive text generation (Markov chains, letter scheduling).
  `rng.rs` is the xorshift32 PRNG. `model.rs` reads keybr.com phonetic model
  files.
- `src/metrics/`: Per-key statistics. `confidence.rs` computes key confidence
  from typing speed.
- `src/import/`: keybr.com data export import (schema, replay, report).
- `src/persistence/`: Saving/loading stats (`stats.rs`), the local-day
  helpers (`localtime.rs`), how user files are written and read
  (`files.rs`: every write is an atomic replace, and a file that exists but
  will not read or parse is moved to `<name>.corrupt-<unix-secs>` instead of
  being overwritten), and the background writer (`saver.rs`). While the TUI
  runs, `App::flush_pending_saves` serializes on the UI thread and queues the
  bytes on `Saver`, whose thread writes every save. The one other disk write,
  a language switch moving a broken stats file aside, runs on the UI thread
  after `Saver::flush`, while the writer is idle. `main` waits for the saver
  on quit and prints its failures after `tui::restore()`. A panic hook
  installed in `main` waits up to 2 s for queued saves, since release builds
  abort on panic and would otherwise drop them.
- `src/config.rs`: User settings. It owns `config.toml` (its format and
  path), not `persistence/`, and reads and writes it through `files.rs`.
- `src/events.rs`: Terminal event channel (keyboard input, tick events).
- `src/tui.rs`: Terminal setup and teardown.
- `src/golden_tests.rs`: Golden tests. They pin lesson text, scheduler
  decisions, import replay and the `stats.json` and `config.toml` formats.
  Never change an expected value in a refactor: a diff there is a behavior
  change.

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

**Always commit `Cargo.lock` alongside `Cargo.toml` changes.** This is a binary
crate — the lock file ensures reproducible builds and is required for
`cargo publish` and `cargo install --locked`.

## Releasing a New Version

Mirrors the whisrs release process (see `~/Projects/whisrs/CLAUDE.md`), with
keybr-tui specifics noted.

**Versioning**: incremental feature/fix releases bump PATCH on the current
minor line (like whisrs's v0.1.x train — currently v0.2.x here). Reserve a
MINOR bump for milestone-level changes (new practice modes, multi-language).
crates.io versions are immutable — never retag or republish a shipped version.

1. **Bump version** in `Cargo.toml`, `flake.nix`, and `contrib/PKGBUILD`
   (`pkgver`); run `cargo build` so `Cargo.lock` picks up the new version,
   and commit the lock file with the bump.
2. **Update `CHANGELOG.md`** — new `## [X.Y.Z] - YYYY-MM-DD` section
   (Keep a Changelog format: Added / Changed / Fixed).
3. **Run all CI checks** locally (see above).
4. **Commit** as `chore(release): vX.Y.Z` and **push** to main.
5. **Tag and push**: `git tag -a vX.Y.Z -m "vX.Y.Z — summary"; git push origin vX.Y.Z`.
   The Release workflow builds binaries for 4 targets (Linux x86_64,
   macOS x86_64/aarch64, Windows x86_64) and creates the GitHub release.
6. **Write the release notes by hand** from the CHANGELOG section
   (`gh release edit vX.Y.Z --notes ...`) — auto-generated notes are
   disabled, same as whisrs, because they miss stacked PRs and read poorly.
7. **Publish to crates.io**: always run `cargo publish` after the tag —
   do not skip this step.
8. **Landing page (gh-pages)**: bump the hardcoded `"softwareVersion"` in
   `index.html` (JSON-LD) and `<lastmod>` in `sitemap.xml` on the
   `gh-pages` branch — use a temporary worktree and push. Never touch
   `install.sh` there (auto-synced by `sync-installer.yml`).
9. **Installer**: nothing to do — `install.sh` always serves the latest
   release and is auto-synced to gh-pages by `sync-installer.yml`.
10. **AUR**: not yet published. `contrib/PKGBUILD` is the in-repo template;
   when an AUR package exists it will be maintained in an external checkout
   (like `~/Projects/whisrs-git`), bumping `pkgver`, regenerating `.SRCINFO`
   with `makepkg --printsrcinfo > .SRCINFO`, and pushing to AUR.
