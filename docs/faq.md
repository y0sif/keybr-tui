# Frequently Asked Questions

## What is keybr-tui?

keybr-tui is a terminal-native typing trainer built in Rust with ratatui. It uses the adaptive Markov-chain text generation algorithm from keybr.com to schedule practice around the keys you are weakest on.

## How is keybr-tui different from keybr.com?

keybr-tui is a faithful port of the keybr.com adaptive algorithm to a terminal UI. It runs entirely offline, stores your stats on disk in plain files, and has no account, browser, or network requirement.

## Does keybr-tui work offline?

Yes. keybr-tui never makes a network request. Text is generated locally by the phonetic Markov model, and all metrics are written to local files.

## Where is my progress saved?

Per-key stats and session history live under your platform's XDG data directory, typically `~/.local/share/keybr-tui/` on Linux. You can confirm the resolved path by running `keybr-tui --data-dir`.

## Can I reset my stats?

Yes. Delete the data directory (or just the stats file inside it) and keybr-tui will start a fresh profile on the next launch. There is no in-app "wipe" command yet.

## What terminal emulators are supported?

Any modern emulator that supports a true-color or 256-color ANSI palette and a monospace font: Alacritty, Kitty, WezTerm, foot, iTerm2, Windows Terminal, GNOME Terminal, and others. Crossterm handles the platform differences.

## Why does keybr-tui keep showing me the same letter?

That is the adaptive algorithm working as designed. keybr-tui identifies your lowest-confidence key — the "focus key" — and biases generated text toward it until your accuracy and speed on that key catch up to the rest.

## How do I change my target WPM?

Edit `~/.config/keybr-tui/config.toml` and set the `target_wpm` field. The config file is not auto-created on first run; see the README's configuration section for a template.

## What does "forgive mistakes" vs "stop on error" do?

In `move-on` mode (forgive mistakes), an incorrect keystroke is recorded but the cursor still advances, so a typo does not block the rest of the line. In `stop-on-error` mode, the cursor refuses to move until you type the correct character, which trains accuracy more strictly.

## Can I use a Dvorak or Colemak layout?

Typing itself is layout-agnostic — keybr-tui reads whatever character your OS sends. The MVP unlocks letters in the QWERTY frequency order from keybr.com, so the *schedule* assumes QWERTY; layout-aware unlock orders are planned for post-MVP.

## Does it support languages other than English?

Not yet. The phonetic Markov model bundled with the MVP is English-only. Non-English phonetic models and locale-specific letter unlock orders are tracked as post-MVP work.

## How can I contribute?

Read [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the repository root for the development setup, coding conventions, and pull-request workflow. Bug reports and small fixes are welcome via GitHub issues at https://github.com/y0sif/keybr-tui/issues.
