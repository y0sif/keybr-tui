# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- taria integration: the app binds a [taria](https://github.com/y0sif/taria)
  socket and publishes a semantic tree for the menu, typing, progress, and
  settings screens, so terminal agents can read and drive the app. Semantic
  acts and the raw-key fallback both lower into the existing key handler, so
  agents cannot reach states a keyboard cannot, and a bind failure falls back
  to running taria-free. Depends on the published `taria-ratatui` 0.2 crate.
- Typed text (taria's `type_text`) is scored as keystrokes against the current
  lesson instead of being lowered through the key handler. This app binds bare
  letters as commands on every screen, so lowered text would have met them:
  `type_text("quick")` on the menu would have quit the app at its first
  character and reported success. Text sent while no lesson is running is
  acknowledged `Ignored`, control characters in the payload are skipped rather
  than lowered (a tab is the typing screen's error-mode toggle), and one call
  types at most one lesson, because finishing one immediately generates the
  next.
- Agent input the app deliberately does nothing with is now acknowledged
  `Ignored` — an act on the wrong screen, an unknown node id, an action a row
  does not advertise, an unparseable key — so an agent waiting on an effect
  stops waiting instead of timing out.
- The typing screen publishes three dashboard rows that were previously
  invisible to agents: the all-keys confidence heatmap (a `chart`, whose
  meaning was carried entirely by colour), the daily-goal bar (a
  `progress_bar`), and the "letter unlocked!" callout (a `status`, which
  clears itself and so could vanish between two reads).
- Agent traffic that went nowhere is reported on stderr after the terminal is
  restored: dropped, stale and unreadable inputs, and answers the bridge was
  too slow to read.

### Changed

- Migrated from ratatui 0.29 to 0.30.2 (dropping the direct crossterm
  dependency).
- MSRV raised from 1.75 to 1.88.

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
