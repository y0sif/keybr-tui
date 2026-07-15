```
 _              _
| | _____ _   _| |__  _ __
| |/ / _ \ | | | '_ \| '__|
|   <  __/ |_| | |_) | |
|_|\_\___|\__, |_.__/|_|
          |___/

  practice the keys that slow you down
```

# keybr-tui

[![CI](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/keybr-tui.svg)](https://crates.io/crates/keybr-tui) [![docs.rs](https://img.shields.io/docsrs/keybr-tui)](https://docs.rs/keybr-tui) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

**Adaptive touch-typing practice for the terminal.** keybr-tui times every key you press, finds the ones slowing you down, and builds drills aimed straight at them. It is a faithful port of the [keybr.com](https://www.keybr.com) learning algorithm, runs entirely offline, and can [import your keybr.com history](#bring-your-keybrcom-history).

## Install

```bash
curl -sSf https://y0sif.github.io/keybr-tui/install.sh | sh
```

The script installs the latest prebuilt binary for your platform. Prefer Cargo? Run `cargo install keybr-tui`.

<details>
<summary>Other ways to install (prebuilt binary, from source)</summary>

**Prebuilt binary.** Every [release](https://github.com/y0sif/keybr-tui/releases) ships binaries for Linux (x86_64), macOS (Intel and Apple Silicon), and Windows (x86_64). Unix archives are `.tar.gz`, Windows is `.zip`.

```bash
tar -xzf keybr-tui-x86_64-unknown-linux-gnu.tar.gz
./keybr-tui
```

**From source.**

```bash
git clone https://github.com/y0sif/keybr-tui.git
cd keybr-tui
cargo install --path .
```

Pin a version for the install script with `KEYBR_TUI_VERSION=v0.2.0`.

</details>

## How it adapts

Every session feeds a per-key model of how fast and accurately you type. keybr-tui reads that model to decide what you practice next.

1. **Start small.** You begin with a handful of letters, not the whole keyboard.
2. **Earn the rest.** A new letter unlocks only once your active set reaches a confidence threshold, measured against your target speed.
3. **Drill the weak spot.** The slowest key in your active set becomes the focus key and shows up more often in the text you are given.
4. **Read like words.** A phonetic Markov chain builds pronounceable pseudo-words from your active letters, so practice never feels like random noise.
5. **Learn from each key.** Every keystroke's reaction time updates a smoothed per-key average, the same calculation keybr.com runs.

Because the engine is a faithful port of the [keybr.com](https://github.com/aradzie/keybr.com) algorithm, the practice you get in the terminal matches what the website would give you.

## Features

| Feature | What you get |
|---|---|
| **Adaptive drills** | Phonetic Markov text built from your active letters, biased toward your weakest key |
| **Per-key tracking** | Reaction times smoothed into a confidence score for every key |
| **Progressive unlock** | Letters open up as you prove proficiency, in keybr.com's frequency order |
| **keybr.com import** | Bring your full learning state over from a keybr.com data export |
| **Two error modes** | Forgive mistakes and keep moving, or stop until you correct them |
| **Lesson summary** | WPM, accuracy, and your weakest keys after every round |
| **Local and offline** | No account, no network, stats saved as plain local files |
| **Terminal-native** | Minimal ratatui interface in ANSI colors, no chrome in the way |

## Usage

```bash
keybr-tui [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--target-wpm <N>` | Target typing speed in words per minute (default: 35) |
| `--error-mode <MODE>` | `move-on` (default) or `stop-on-error` |
| `--reset` | Delete saved stats and start fresh |
| `--data-dir` | Print the data directory path and exit |
| `--import <FILE>` | Import a keybr.com data export and exit (see below) |
| `--force` | With `--import`, replace existing stats (a `.bak` backup is kept) |
| `--help` | Show help |
| `--version` | Show version |

**In-session keys:** `Esc` returns to the menu or quits, `Enter` selects a menu item or dismisses the lesson summary, the arrow keys navigate menus and settings, and `Left`/`Right` adjust setting values.

## Bring your keybr.com history

Already practicing on [keybr.com](https://www.keybr.com)? Carry your progress over and continue in the terminal.

1. On keybr.com, open your profile and click **Download data**. You get a `typing-data.json` file with your full practice history.
2. Import it:

   ```bash
   keybr-tui --import typing-data.json
   ```

The importer replays every session through the same per-key smoothing keybr.com uses, so your unlocked letters, per-key speeds, and focus letter come out exactly as the website computed them.

A few details worth knowing:

- Your target speed is not part of the export. If you use a non-default target on keybr.com, pass `--target-wpm <N>` alongside `--import`, since the unlocked set is derived against it. With no flag, the target saved in your config is used.
- Sessions from non-English layouts are skipped, since this TUI is English-only for now.
- Existing local stats are never overwritten unless you pass `--force`, which still writes a `stats.json.bak` backup first.

## Configuration

keybr-tui runs with zero config. To change the defaults, edit `~/.config/keybr-tui/config.toml` (the XDG path on Linux):

```toml
target_wpm = 35
error_mode = "forgive-mistakes"  # or "stop-on-error"
fragment_length = 100
```

<details>
<summary>All config options</summary>

| Key | Default | Description |
|---|---|---|
| `target_wpm` | `35` | Target speed in words per minute; drives the confidence threshold |
| `error_mode` | `"forgive-mistakes"` | `"forgive-mistakes"` advances past typos, `"stop-on-error"` blocks until you correct them |
| `fragment_length` | `100` | Characters of practice text generated per lesson |
| `natural_words` | `true` | Prefer real English words, falling back to the phonetic model when none fit your active letters |
| `daily_goal_minutes` | `30` | Daily practice goal in minutes; `0` hides the daily-goal indicator |
| `alphabet_size` | `0.0` | Fraction of the non-starter alphabet to force-unlock regardless of confidence (keybr's `alphabetSize`), clamped to the range 0.0 to 1.0 |
| `focus_letter` | `unset` | Pins one unlocked letter, given as a single letter (a-z), so every generated word contains it; omit the key (or cycle the Settings "Focus letter" row to Auto) for the automatic weakest-key focus |

</details>

Stats are stored separately. Run `keybr-tui --data-dir` to find them.

## Documentation

- [docs/comparison.md](docs/comparison.md): how keybr-tui compares to other typing trainers
- [docs/faq.md](docs/faq.md): frequently asked questions
- [docs/troubleshooting.md](docs/troubleshooting.md): common issues and fixes
- [CONTRIBUTING.md](CONTRIBUTING.md): building from source, project layout, and the contribution workflow

## Credits

Algorithm inspired by [keybr.com](https://www.keybr.com) by [aradzie](https://github.com/aradzie/keybr.com). Built with [ratatui](https://ratatui.rs/) and [crossterm](https://github.com/crossterm-rs/crossterm).

## License

[MIT](LICENSE)
