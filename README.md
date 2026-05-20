# keybr-tui

[![CI](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/y0sif/keybr-tui/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/keybr-tui.svg)](https://crates.io/crates/keybr-tui) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

A terminal typing trainer with adaptive learning, inspired by [keybr.com](https://www.keybr.com).

keybr-tui generates practice text using phonetic Markov chains and adapts to your weaknesses in real time. Letters are introduced progressively as you demonstrate proficiency, so you always practice what you need most.

## Features

- **Adaptive text generation** using phonetic Markov chains (faithful port of the keybr.com algorithm)
- **Per-key confidence tracking** with exponential smoothing of reaction times
- **Progressive letter unlocking** based on your performance against a target speed
- **Persistent progress** across sessions (stats and config saved automatically)
- **Backspace and error recovery** with two error modes (forgive mistakes / stop on error)
- **Lesson summary** after each practice round showing WPM, accuracy, and weakest keys
- **Progress view** to review per-key statistics
- **Configurable settings** (target WPM, error mode, fragment length)
- **Minimalist terminal-native UI** built with ratatui

## Install

### From crates.io

```bash
cargo install keybr-tui
```

### Prebuilt binary

Each tagged release on the [releases page](https://github.com/y0sif/keybr-tui/releases) ships prebuilt binaries for Linux (x86_64), macOS (Intel and Apple Silicon), and Windows (x86_64). Unix archives are `.tar.gz`, Windows is `.zip`.

```bash
tar -xzf keybr-tui-x86_64-unknown-linux-gnu.tar.gz
./keybr-tui
```

### From source

```bash
git clone https://github.com/y0sif/keybr-tui.git
cd keybr-tui
cargo install --path .
```

## Usage

```bash
keybr-tui [OPTIONS]
```

### Options

| Flag | Description |
|------|-------------|
| `--target-wpm <N>` | Set target typing speed in words per minute (default: 35) |
| `--error-mode <MODE>` | `move-on` (default) or `stop-on-error` |
| `--reset` | Delete saved stats and start fresh |
| `--data-dir` | Print the data directory path and exit |
| `--help` | Show help |
| `--version` | Show version |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Esc` | Return to menu / quit |
| `Enter` | Select menu item / dismiss lesson summary |
| Arrow keys | Navigate menus and settings |
| `Left`/`Right` | Adjust settings values |

## How the Adaptive Algorithm Works

keybr-tui uses a phonetic text generation algorithm ported from [keybr.com](https://github.com/aradzie/keybr.com):

1. **Letter scheduling**: You start with a small set of letters (6). The scheduler tracks your per-key reaction time using exponential smoothing and computes a confidence score against your target speed.
2. **Unlocking**: When all active letters reach sufficient confidence, a new letter is unlocked from a frequency-ordered list.
3. **Focus key**: The weakest key among your active set becomes the "focus key" and appears more frequently in generated text.
4. **Text generation**: A Markov chain trained on English phonetic patterns generates pronounceable pseudo-words using only your active letters, with bias toward the focus key.
5. **Tracking**: Each keystroke's reaction time is recorded, filtered, and smoothed to update your per-key statistics.

## Project Structure

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
├── docs/                 # Architecture and reference
└── .github/workflows/    # CI and release
```

See [docs/file_structure.md](docs/file_structure.md) for the full module responsibility breakdown.

## Development

Build:

```bash
cargo build
```

Run:

```bash
cargo run
```

Test:

```bash
cargo test
```

Format:

```bash
cargo fmt --all
```

Lint:

```bash
cargo clippy --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution workflow.

## Configuration

Config file location (XDG on Linux):

```text
~/.config/keybr-tui/config.toml
```

Example config:

```toml
target_wpm = 35
error_mode = "forgive-mistakes"  # or "stop-on-error"
fragment_length = 100
```

Stats are saved separately in the data directory. Use `keybr-tui --data-dir` to find it.

## Screenshots

A terminal recording (asciinema/vhs) is on the roadmap. For now, run `cargo install keybr-tui` and try it.

## Documentation

- [docs/project_overview.md](docs/project_overview.md) — scope, goals, and tech stack
- [docs/architecture.md](docs/architecture.md) — MVU architecture and event loop
- [docs/file_structure.md](docs/file_structure.md) — module-by-module responsibilities
- [docs/comparison.md](docs/comparison.md) — how keybr-tui compares to alternatives
- [docs/faq.md](docs/faq.md) — frequently asked questions
- [docs/troubleshooting.md](docs/troubleshooting.md) — common issues and fixes

## Credits

- Algorithm inspired by [keybr.com](https://www.keybr.com) by [aradzie](https://github.com/aradzie/keybr.com)
- Built with [ratatui](https://ratatui.rs/) and [crossterm](https://github.com/crossterm-rs/crossterm)

## License

[MIT](LICENSE)
