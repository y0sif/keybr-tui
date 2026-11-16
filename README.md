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

[![CI](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/keybr-tui.svg)](https://crates.io/crates/keybr-tui) [![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE) [![MSRV](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

**Adaptive TUI typing trainer in Rust, bringing keybr.com's algorithm to your terminal. Offline touch-typing practice that imports your keybr.com data export.**

keybr-tui times every key you press, finds the ones that slow you down, and builds lessons around them. Already practicing on keybr.com? [Bring your keybr.com history](#bring-your-keybrcom-history) and continue where you left off.

<p align="center"><img src="assets/demo.gif" alt="keybr-tui demo: a typing lesson in the terminal, then per-lesson speed and accuracy, the per-key heatmap and a new letter unlocking, and the per-key progress screen" width="900"></p>

## Install

```bash
curl -sSf https://y0sif.github.io/keybr-tui/install.sh | sh
```

The script installs the latest release on Linux (x86_64) and macOS (Intel and Apple Silicon), into `/usr/local/bin` when it is writable and `~/.local/bin` otherwise. On Windows, use Cargo or the prebuilt zip below.

<details>
<summary>Other ways to install: Cargo, prebuilt binaries, Nix, from source</summary>

**Cargo:** `cargo install keybr-tui --locked`

**Prebuilt binaries:** every [release](https://github.com/y0sif/keybr-tui/releases) has archives for Linux x86_64, macOS (Intel and Apple Silicon) and Windows x86_64 (`.zip`). Extract one and run `keybr-tui`.

**Nix:** `nix run github:y0sif/keybr-tui`, or `nix profile install github:y0sif/keybr-tui` to keep it.

**From source:** clone the repository, then run `cargo install --path . --locked` inside it.

**A specific version:** put `KEYBR_TUI_VERSION` with a tag from the releases page before `sh`:

```bash
curl -sSf https://y0sif.github.io/keybr-tui/install.sh | KEYBR_TUI_VERSION=vX.Y.Z sh
```

</details>

## How it adapts

keybr-tui keeps a per-key model of your speed and accuracy and uses it to decide what you practice next.

1. **Start small.** You begin with six letters, not the whole keyboard.
2. **Earn the rest.** A new letter unlocks once every letter you have reaches your target speed.
3. **Drill the weak spot.** Your slowest letter becomes the focus key and appears in every word. You can pin a letter yourself, or in English a combination drill such as `cr` or `-tion`, in [Settings](docs/configuration.md#settings-screen).
4. **Practice real words.** Lessons use real words of your language that fit your letters, with pronounceable pseudo-words from keybr.com's phonetic model filling in when too few fit.
5. **Learn from each lesson.** When a lesson ends, each key's average time in it updates that key's smoothed speed, the same calculation keybr.com runs.

## Features

keybr-tui runs keybr.com's adaptive lessons and progress tracking in your terminal.

| Feature | What you get |
|---|---|
| **keybr.com import** | Your unlocked letters and key speeds from a keybr.com data export |
| **27 languages** | keybr.com's word list, phonetic model and letter order for each |
| **Live dashboard** | Last lesson's speed and accuracy, a per-key heatmap and a daily goal above the text |
| **Progress screen** | Speed, best, attempts, errors and status for every letter |
| **Two error modes** | Forgive mistakes and keep moving, or stop until you correct them |
| **Local and offline** | No account, no network, stats saved as plain local files |

## Usage

Run `keybr-tui` to start practicing; every flag below is optional.

```bash
keybr-tui [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--target-wpm <N>` | Target typing speed in words per minute (default: 35) |
| `--error-mode <MODE>` | `stop-on-error` (or `stop`) holds the cursor on a mistake; `move-on` forgives it |
| `--reset` | Delete the practice language's saved stats and start fresh |
| `--data-dir` | Print the folder your stats are saved in and exit |
| `--import <FILE>` | Import a keybr.com data export and exit (see below) |
| `--force` | With `--import`, replace existing stats (a `.bak` backup is kept) |
| `--layout <LAYOUT>` | With `--import`, the keybr.com layout family to import: a family (`colemak`), a layout id (`en-colemak`) or `all`; default: the family of your latest session |
| `--language <CODE>` | Practice language: a code from [Languages](#languages) (default: `en`) or the language's name; saved to the config |
| `--help` | Show help |
| `--version` | Show version |

| Screen | Keys |
|---|---|
| Menu | `Up`/`Down` move, `Enter` selects, `q` or `Esc` quits |
| Typing | `Esc` returns to the menu, `Tab` switches the error mode, `Backspace` steps back one character and clears its mistake |
| Progress | `Up`/`Down`, `PageUp`/`PageDown` and `Home`/`End` scroll; `Esc` returns to the menu |
| Settings | `Up`/`Down` pick a row, `Left`/`Right` change it; `Esc` returns to the menu |

`Ctrl+C` quits from any screen. A finished lesson rolls straight into the next one.

## Bring your keybr.com history

keybr-tui imports a keybr.com data export and continues from the same learning state. On keybr.com, open your profile and click **Download data** to get `typing-data.json`, then run:

```bash
keybr-tui --import typing-data.json
```

The import replays your sessions the way keybr.com does, so your unlocked letters, key speeds and focus letter match the website. It takes the sessions of your practice language (`--language` picks another) in one layout family, by default the family of your latest session.

- `--layout colemak` imports another family, and `--layout all` merges every family into one set of key stats.
- The export does not record your target speed. If yours on keybr.com differs from the one in your config, pass it with `--target-wpm <N>`, since unlocks depend on it.
- Existing stats are kept unless you pass `--force`, which saves a `.bak` copy first.
- English (UK) and Croatian cannot be imported: keybr.com records their sessions under another language's layouts.

## Languages

Switch the practice language with the Language row in Settings, or at launch with `--language fr` (a name such as `français` works too). Each language keeps its own stats, and keybr-tui remembers your choice.

| Code | Language | Code | Language | Code | Language |
|---|---|---|---|---|---|
| `en` | English | `fi` | Finnish (Suomi) | `nl` | Dutch (Nederlands) |
| `en-gb` | English (UK) | `fr` | French (Français) | `pl` | Polish (Polski) |
| `be` | Belarusian (Беларуская) | `hr` | Croatian (Hrvatski) | `pt` | Portuguese (Português) |
| `cs` | Czech (Čeština) | `hu` | Hungarian (Magyar) | `ro` | Romanian (Română) |
| `da` | Danish (Dansk) | `hy` | Armenian (Հայերեն) | `ru` | Russian (Русский) |
| `de` | German (Deutsch) | `it` | Italian (Italiano) | `sl` | Slovenian (Slovenščina) |
| `el` | Greek (Ελληνικά) | `lt` | Lithuanian (Lietuvių) | `sv` | Swedish (Svenska) |
| `es` | Spanish (Español) | `lv` | Latvian (Latviešu) | `tr` | Turkish (Türkçe) |
| `et` | Estonian (Eesti) | `nb` | Norwegian Bokmål (Norsk bokmål) | `uk` | Ukrainian (Українська) |

## Configuration

keybr-tui needs no configuration: change settings on the Settings screen (main menu, **Settings**). Settings live in `config.toml`, which also holds a few options the screen does not show, such as the daily goal. [docs/configuration.md](docs/configuration.md) lists every setting, every key, and the file's path on each OS.

## Documentation

- [docs/configuration.md](docs/configuration.md): every setting and `config.toml` key
- [docs/comparison.md](docs/comparison.md): how keybr-tui compares to other typing trainers
- [docs/faq.md](docs/faq.md): frequently asked questions
- [docs/troubleshooting.md](docs/troubleshooting.md): common issues and fixes
- [CONTRIBUTING.md](CONTRIBUTING.md): building from source, project layout, and the contribution workflow

## Credits

keybr-tui ports the learning algorithm of [keybr.com](https://www.keybr.com) by [aradzie](https://github.com/aradzie/keybr.com) and ships its phonetic models, word lists and keyboard layout table. See [data/README.md](data/README.md) for where each file comes from. Built with [ratatui](https://ratatui.rs/) and [crossterm](https://github.com/crossterm-rs/crossterm).

## License

Copyright (C) 2026 keybr-tui contributors. keybr-tui is licensed under the [GNU Affero General Public License v3.0 only](LICENSE) because it includes and adapts AGPLv3 material from [keybr.com](https://github.com/aradzie/keybr.com). See [NOTICE](NOTICE) for third-party notices.
