# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Daily goal now rolls over at local midnight instead of UTC midnight. Previously an
  evening session west of UTC was stamped with tomorrow's date, so the goal bar started
  the next morning already (partly) full. The local offset comes from `libc::localtime_r`.
  libc is a new dependency, declared under `[target.'cfg(unix)'.dependencies]` — crossterm
  declares its own libc as optional and unix-gated, so it is not a dependency crossterm
  supplies on every target.
- `--import` now credits a keybr.com session to the local day it actually happened on.
  Export timestamps are UTC, so matching them against the (now local) date string dropped
  practice seconds from `today_seconds_practiced` for anyone not on UTC — an early-morning
  session east of UTC, or an evening one west of it. The import instead compares
  each timestamp against the UTC window `[local midnight, local midnight + 24h)`.
- Both of the above apply to Linux and macOS only. On Windows nothing changes: the local
  offset is read through `localtime_r`, which the Windows build does not use, so it falls
  back to UTC — the daily goal there still rolls over at UTC midnight and `--import` still
  credits sessions to the UTC day, exactly as before.

## [0.2.2] - 2026-07-23

### Added

- Manual letter focus (#7): a "Focus letter" row in Settings pins one unlocked letter
  so every generated word contains it, overriding the automatic weakest-key focus.
  Left/Right cycles Auto plus the currently unlocked letters. The pin persists in
  `config.toml` as `focus_letter` (lowercased and validated against unlocked letters
  at load, falling back to Auto if invalid), and the pinned key renders reversed in
  the key heatmap to distinguish it from the automatic focus.

## [0.2.1] - 2026-06-22

### Changed

- Rewrote the README with its own identity instead of mirroring sibling projects:
  a tighter intro, a "How it adapts" section leading with the adaptive engine,
  collapsible install methods, and a full config-option reference. Development and
  project-layout notes moved to `CONTRIBUTING.md`.
- Keyword-dense crate metadata (`description`, `keywords`, `homepage`) for crates.io
  and search discoverability.

### Added

- SEO landing page at <https://y0sif.github.io/keybr-tui/> with structured data
  (`SoftwareApplication` and `FAQPage`), a sitemap, and Google Search Console
  verification.

## [0.2.0] - 2026-06-12

### Added

- Import your keybr.com practice history: `keybr-tui --import typing-data.json` replays
  the full export through keybr's own algorithm (per-key EMA over session means,
  historical-best tracking, validity filtering) and reconstructs your unlocked letters,
  per-key speeds, and focus letter so you can continue in the terminal where the website
  left off. `--force` replaces existing stats and always writes a `stats.json.bak` backup.
- The import summary states which target speed the unlock derivation used, since the
  unlocked set is re-derived against the current target (just like keybr.com).
- `alphabet_size` setting (Settings screen + config.toml), mirroring keybr.com's
  alphabetSize: force-include up to 20 extra letters beyond the starter six, regardless
  of confidence. Defaults to 0 (pure earn-by-confidence progression).

### Fixed

- Per-key smoothing now matches keybr.com exactly: the exponential moving average is
  applied once per lesson to each key's mean latency, not on every keystroke (which
  adapted roughly 10x too fast).
- Focus-key selection now matches keybr.com: the weakest active key (lowest historical-best
  confidence below target) is boosted, instead of the first below-target key in unlock
  order — and once every active key is learned there is no boosted key at all.
- Running `cargo test` no longer overwrites your real config and stats: the update layer
  only raises save flags now, and the main event loop is the single place user files are
  written.

## [0.1.0] - 2026-03-29

### Added

- Adaptive phonetic text generation using Markov chains (faithful port of the keybr.com algorithm)
- Per-key confidence tracking with exponential smoothing of reaction times
- Progressive letter unlocking based on performance against target speed
- Focus key system that biases text generation toward your weakest letter
- Backspace support with two error modes: forgive mistakes and stop on error
- Real-time WPM and accuracy display during typing sessions
- Lesson summary screen showing WPM, accuracy, newly unlocked letters, and weakest keys
- Main menu with navigation to typing practice, progress view, and settings
- Progress view displaying per-key statistics (speed, confidence, attempts, errors)
- Settings screen for adjusting target WPM, error mode, and fragment length
- Persistent stats saved automatically between sessions (JSON format)
- Persistent configuration via TOML config file
- CLI arguments: `--target-wpm`, `--error-mode`, `--reset`, `--data-dir`
- Minimalist terminal UI built with ratatui and crossterm
- ANSI-only color palette for universal terminal compatibility
