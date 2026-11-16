# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Agents using `taria-mcp` 0.4 get an answer within one frame to an input that
  changes nothing. keybr-tui now uses `taria-ratatui` 0.4.
- Launching no longer prints the taria socket path, which showed up after exit.
  If taria fails to start, that is still reported.

### Fixed

- A crash or a closed terminal during a save could leave the stats file empty
  and lose all progress. The stats and config files now always hold either the
  old or the new contents.
- A stats or config file that cannot be read or parsed is no longer overwritten
  by the next save. It is kept as `<name>.corrupt-<time>` next to the original,
  and the warning says where.
- A save that fails (a full disk, say) was printed under the full-screen view,
  where nobody could see it. Failures are now reported after exit, and keybr-tui
  exits with a non-zero status.
- A slow disk no longer makes the app stall at the end of a lesson, on a
  settings change or when leaving a lesson. Saves now happen in the background.

## [0.3.1] - 2026-11-06

### Changed

- License changed from MIT to AGPL-3.0-only. keybr-tui ports the keybr.com
  algorithm and ships keybr.com's phonetic models, word lists and keyboard
  layout table. That material is AGPLv3, so keybr-tui now carries the same
  license. Earlier releases were labelled MIT but already included that material.
- Release archives now ship LICENSE, NOTICE and README.md next to the binary.

## [0.3.0] - 2026-10-20

### Added

- Turkish practice language (`--language tr` or `language = "tr"` in
  `config.toml`). It uses keybr.com's Turkish phonetic model, word list and
  letter order, shows all 29 letters in the heatmap with Turkish case rules
  (`i` → `İ`, `ı` → `I`), and keeps its stats in its own `stats-tr.json`.
  `--import` replays the Turkish sessions of a keybr.com export when Turkish is
  the practice language.
- German practice language (`--language de` or `language = "de"` in
  `config.toml`). It uses keybr.com's German phonetic model, word list and
  letter order (starting `e n t i r l`), shows all 30 letters in the heatmap
  (`ß` as `ẞ`), and keeps its stats in its own `stats-de.json`. `--import`
  replays the German sessions of a keybr.com export when German is the
  practice language.
- 24 more practice languages, for 27 in all: Armenian, Belarusian, Croatian,
  Czech, Danish, Dutch, English (UK), Estonian, Finnish, French, Greek,
  Hungarian, Italian, Latvian, Lithuanian, Norwegian Bokmål, Polish, Portuguese,
  Romanian, Russian, Slovenian, Spanish, Swedish and Ukrainian. Each uses
  keybr.com's data, keeps its stats in its own `stats-<code>.json`, and is
  chosen with `--language` by code (`fr`, `en-gb`) or by name in English or in
  the language itself (`french`, `français`). Greek tiles keep their accented
  capitals and show final sigma as `ς`, so every tile is distinct, and Armenian
  `և` shows as `ԵՎ`. Only English (UK) shares the English combination drills,
  and `--import` finds no sessions for English (UK) or Croatian, which have no
  keybr.com layouts of their own.
- A Language row in Settings, the last row, cycling through the practice
  languages by their own names (English, Türkçe, Deutsch, Français and so on)
  with Left/Right. Switching takes effect at once, like relaunching with
  `--language`: your stats are saved, the chosen language's own stats load,
  the next lesson is in that language, and `config.toml` remembers the choice.
  `--import` and `--reset` still act on the language from `--language` or
  `config.toml`.
- `--layout <LAYOUT>` for `--import`: the keybr.com layout family to import,
  given as a family (`colemak`), a layout id standing for its family
  (`en-colemak`), or `all`. Sessions are matched to their language and family
  with keybr.com's own layout table.

### Changed

- English practice, its files and its generated text are unchanged by the new
  languages.
- `--import` now imports one layout family, as keybr.com keeps key stats per
  family: by default the family of your most recent session in the practice
  language. If your export has sessions on several families (QWERTY and
  Colemak, or Turkish Q and Turkish F), only the latest is imported, and
  `--layout all` merges them. The report names the family imported and the
  sessions skipped.
- Natural-words lessons skip 2-letter words and draw from the 1,000 most
  common words that fit your letters.

### Fixed

- The key heatmap wraps onto balanced rows on narrow terminals instead of
  being cut off on the right.
- The Progress screen scrolls when its letter list is taller than the
  terminal, instead of cutting off the letters past the bottom. Up/Down,
  PageUp/PageDown, Home and End move it, a line below the list says how many
  letters are above and below, and the footer shows `[Up/Down] Scroll`.
- Early lessons in languages where few real words fit the first letters no
  longer cycle a handful of words. When fewer than 15 real words fit,
  generated words fill in, as on keybr.com.

## [0.2.4] - 2026-09-29

### Fixed

- The daily goal rolls over at local midnight on Windows too, and `--import`
  credits keybr.com sessions to the local day there, daylight saving time
  included ([#11](https://github.com/y0sif/keybr-tui/issues/11)).
- `cargo install` works on Solaris, illumos, AIX, VxWorks, QuRT and newlib
  targets ([#12](https://github.com/y0sif/keybr-tui/issues/12)). There the
  daily goal rolls over at UTC midnight.

## [0.2.3] - 2026-09-29

### Added

- taria integration: the app publishes its menu, typing, progress and settings
  screens over a [taria](https://github.com/y0sif/taria) socket, so terminal
  agents can read and drive it. Agent input goes through the same key handling
  as the keyboard, so an agent cannot reach a state a keyboard cannot. If the
  socket cannot be bound, the app runs without taria. The integration works on
  Linux and macOS; Windows builds have the same screens, keys and files with no
  agent surface.
- Typed text (taria's `type_text`) is scored as keystrokes against the current
  lesson, so it never triggers a menu command. Text sent while no lesson is
  running is acknowledged `Ignored`, control characters in it are skipped, and
  one call types at most one lesson.
- Agent input the app does nothing with (an act on the wrong screen, an
  unknown node id, an action a row does not advertise, an unparseable key) is
  acknowledged `Ignored`, so an agent waiting on an effect stops waiting.
- Agents can read the typing screen's key heatmap (a `chart`), daily-goal bar
  (a `progress_bar`) and "letter unlocked!" callout (a `status`).
- Agent traffic that went nowhere is reported on stderr after exit: dropped,
  stale and unreadable inputs, and answers the bridge was too slow to read.
- Combination drills ([#9](https://github.com/y0sif/keybr-tui/issues/9)): a
  "Focus pattern" row in Settings pins one of 16 preset drills, 8 bigram
  reaches (`cr`, `pl`, `br`, `tr`, `gr`, `fr`, `bl`, `cl`) and 8 word endings
  (`-ing`, `-ed`, `-er`, `-ly`, `-tion`, `-ment`, `-able`, `-ous`), so every
  generated word matches the whole pattern. Left/Right cycles Off plus the
  presets your unlocked letters can fill with at least 8 real words: the row
  reads "None yet" on a fresh profile, offers `-ER` once your seventh letter
  unlocks, and has all 16 by 21 letters. Drills use real words even with
  `natural_words` off. A pattern and a letter pin share one focus slot, so
  picking one clears the other, and the pattern is saved as `focus_pattern` in
  `config.toml`.

### Changed

- Migrated from ratatui 0.29 to 0.30.2.
- MSRV raised from 1.75 to 1.88.

### Fixed

- On Linux and macOS, the daily goal rolls over at local midnight instead of
  UTC midnight. An evening session west of UTC no longer counts toward the
  next day's goal.
- On Linux and macOS, `--import` credits each keybr.com session to the local
  day it happened on.

## [0.2.2] - 2026-07-23

### Added

- Manual letter focus ([#7](https://github.com/y0sif/keybr-tui/issues/7)): a
  "Focus letter" row in Settings pins one unlocked letter so every generated
  word contains it, overriding the automatic weakest-key focus. Left/Right
  cycles Auto plus the unlocked letters. The pin is saved in `config.toml` as
  `focus_letter`, and the pinned key shows reversed in the key heatmap.

## [0.2.1] - 2026-06-22

### Added

- A landing page at <https://y0sif.github.io/keybr-tui/>.

### Changed

- Rewrote the README: a tighter intro, a "How it adapts" section leading with
  the adaptive engine, collapsible install methods, and a full config-option
  reference. Development and project-layout notes moved to `CONTRIBUTING.md`.

## [0.2.0] - 2026-06-12

### Added

- Import your keybr.com practice history: `keybr-tui --import typing-data.json`
  replays the full export through keybr.com's own algorithm and reconstructs
  your unlocked letters, per-key speeds and focus letter, so you can continue
  in the terminal where the website left off. `--force` replaces existing stats
  and always writes a `stats.json.bak` backup.
- The import summary names the target speed used to work out your unlocked
  letters, which are re-derived against the current target, as on keybr.com.
- `alphabet_size` setting (Settings screen and `config.toml`), as on keybr.com:
  force-include up to 20 extra letters beyond the starter six, regardless of
  confidence. Defaults to 0 (letters are earned by confidence alone).

### Fixed

- Per-key smoothing matches keybr.com: it is applied once per lesson to each
  key's mean time, not on every keystroke, which adapted too fast.
- Focus-key selection matches keybr.com: the weakest active key is boosted,
  instead of the first below-target key in unlock order. Once every active key
  is learned, no key is boosted.
- Running `cargo test` no longer overwrites your real config and stats.

## [0.1.0] - 2026-03-29

### Added

- Adaptive phonetic text generation using Markov chains (faithful port of the keybr.com algorithm)
- Per-key confidence tracking with exponential smoothing of reaction times
- Progressive letter unlocking based on performance against target speed
- Focus key system that biases text generation toward your weakest letter
- Backspace support with two error modes: forgive mistakes and stop on error
- Dashboard above each lesson with the last lesson's WPM and accuracy, a key heatmap, the focus key and a daily goal; a finished lesson rolls straight into the next
- Main menu with navigation to typing practice, progress view, and settings
- Progress view displaying per-key statistics (speed, confidence, attempts, errors)
- Settings screen for adjusting target WPM, error mode, and fragment length
- Persistent stats saved automatically between sessions (JSON format)
- Persistent configuration via TOML config file
- CLI arguments: `--target-wpm`, `--error-mode`, `--reset`, `--data-dir`
- Minimalist terminal UI built with ratatui and crossterm
- ANSI-only color palette for universal terminal compatibility
