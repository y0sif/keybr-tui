# keybr-tui: agent guide

keybr-tui is an adaptive typing trainer for the terminal: one Rust binary crate that ports
keybr.com's lesson algorithm to ratatui and embeds keybr.com's phonetic model and word list for
each language it teaches.

## Build and check

```sh
cargo build                                  # debug build
cargo run -- --help                          # run it; the CLI flags
cargo test                                   # unit and golden tests (inline modules, no tests/ dir)
cargo clippy --all-targets -- -D warnings    # lint; warnings are errors
cargo fmt --all                              # format
```

CI runs the gate in [CONTRIBUTING.md#checks](CONTRIBUTING.md#checks), its cargo steps on Linux,
macOS and Windows. `just check` runs the format, clippy and test steps. Rules:

- Never push before every check there passes locally.
- Commit `Cargo.lock` with every `Cargo.toml` change. It is tracked because the crate ships a
  binary (`cargo install --locked`).
- `rust-version` in `Cargo.toml` and the toolchain that `flake.nix` pins move together.

## Layout

| Path | Holds |
|---|---|
| `src/main.rs` | the CLI, startup, the event loop, waiting for saves on quit and on panic |
| `src/app/` | `App`, the Model: `mod.rs` (the struct, constructors), `focus.rs`, `lesson.rs`, `profile.rs` |
| `src/update/` | the Update: `update()` and one key handler per screen; `agent.rs` applies taria input |
| `src/ui.rs`, `src/components/` | the View: `view()` and the widgets it draws, one file per screen or strip |
| `src/screens.rs` | screen structure both sides use: menu items, settings rows, progress table geometry |
| `src/tree.rs` | the taria accessibility tree, a second read-only view for agents |
| `src/engine/` | lesson text: scheduler, filter, generator, dictionary, phonetic model, languages, layouts, PRNG |
| `src/metrics/` | per-key stats (`KeyStats`) and key confidence from typing speed |
| `src/import/` | `--import` of a keybr.com data export: schema, replay, report |
| `src/persistence/` | stats files, local-day helpers, atomic writes (`files.rs`), the background writer (`saver.rs`) |
| `src/config.rs` | `Config` and config.toml: its format, path, load and save |
| `src/events.rs`, `src/tui.rs` | the key, resize and tick event channel; terminal setup and teardown |
| `src/golden_tests.rs` | golden tests (see Rules) |
| `src/*/test_support.rs` | helpers shared by one directory's tests |
| `data/` | the embedded keybr.com models (`model-<code>.data`) and word lists (`wordlist-<code>.txt`) |
| `demo/`, `assets/` | the VHS tape and scripts that record `assets/demo.gif` (see `demo/README.md`) |
| `contrib/PKGBUILD` | Arch Linux package template |
| `docs/` | user docs; only the pages whitelisted in `.gitignore` are tracked |
| `scripts/check-docs.sh` | the docs check (CI) |
| `install.sh` | the `curl \| sh` installer; `sync-installer.yml` mirrors it to the `gh-pages` branch |
| `flake.nix`, `justfile` | the Nix package and dev shell; the task runner |

## Rules

- MVU: `ui.rs`, `components/` and `tree.rs` take `&App` and never write it. `update/` writes
  `App` and never renders or touches the disk: it sets `pending_stats_save`,
  `pending_config_save` or `pending_language_switch`, and `App::flush_pending_saves` acts on them
  in main's loop. `screens.rs` holds what both sides share and no `App` state.
- State: all mutable runtime state lives in `App`. The only statics are the read-only tables
  `SPECS` (`engine/language.rs`) and `LAYOUTS` (`engine/layout.rs`), the write-once letter cache
  `MODEL_LETTERS`, and test-only items. Add no other global.
- Algorithm: the engine mirrors keybr.com, and its doc comments name the upstream files. Lesson
  text comes only from keybr.com's phonetic models and word lists in `data/`: no other word
  source, no random characters.
- Colors: only ratatui's named ANSI colors (`Color::Red`, `Color::DarkGray`, `Color::Reset`),
  never `Color::Rgb` or `Color::Indexed`. They are set only in `components/` and `ui.rs`.
- Unix only: taria (`tree.rs`, `update/agent.rs`, the `taria-ratatui` dependency) and `libc`
  are `cfg(unix)`. Gate every new touch point the same way. Check the Windows build with
  `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings`.
- User files: the stats files and config.toml are written only through
  `persistence::write_atomic` and read only through `read_user_file` (`persistence/files.rs`).
  In the TUI every save is queued on `persistence::Saver`. A test that touches the disk uses
  `persistence::test_support::TestDir`, never the real config or data directory.
- Golden tests (`src/golden_tests.rs`) pin lesson text, scheduler decisions, import replay and
  the `stats.json` and `config.toml` formats. Never change an expected value in a refactor: a
  diff there is a behavior change.

## Languages and data

Every model and word list is compiled in: `SPECS` in `src/engine/language.rs` embeds each one
with `include_bytes!` or `include_str!`, so nothing under `data/` is read at runtime. The source,
normalisation and license of each file are in `data/README.md`, and only there.

- Adding a language: its two files go in `data/` as [data/README.md](data/README.md#adding-a-language) says
  (the `include` list in `Cargo.toml` packs only `model-*.data` and `wordlist-*.txt`). The code
  side is a `Language` variant and a `SPECS` row (the `//!` comment of `engine/language.rs`).
  Then add it to the README's `## Languages` table.
- `LAYOUTS` is keybr.com's layout table. Its `//!` comment says how to regenerate it.

## Docs

- One fact, one home. How to use a feature lives in `README.md` or `docs/` (every setting:
  `docs/configuration.md`). How a subsystem works, and its invariants, live in the module's `//!`
  comment: read it before changing the module. This file holds only the rules above: link to
  the home of a fact instead of repeating it.
- A change to user-visible behavior updates its doc in the same change.
- `docs/` is gitignored except the whitelisted pages: run `git check-ignore -v <path>` before
  adding one.
- No issue numbers, fix history or personal paths in docs or doc comments (those go in the
  commit message), and no internal files (specs, plans, notes) in the repo.
  `scripts/check-docs.sh` checks the docs in CI.
