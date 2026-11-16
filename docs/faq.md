# Frequently Asked Questions

## What is keybr-tui?

keybr-tui is a terminal typing trainer built in Rust with ratatui. It ports keybr.com's adaptive algorithm, which unlocks letters as you master them and builds every lesson around your slowest key.

## How is keybr-tui different from keybr.com?

keybr-tui runs the keybr.com algorithm in a terminal instead of a browser. It works offline, stores your stats on disk in plain files, and needs no account.

## Can I import my keybr.com data into keybr-tui?

Yes: download your data from your keybr.com profile and run `keybr-tui --import typing-data.json`. See [Bring your keybr.com history](../README.md#bring-your-keybrcom-history) for layouts and the other options.

## Does keybr-tui work offline?

keybr-tui never makes a network request: its word lists and phonetic models ship inside the binary, and all stats are written to local files.

## Where is my progress saved?

Run `keybr-tui --data-dir` to print the folder that holds your stats. Each language has its own file there: `stats.json` for English, `stats-<code>.json` for the others (`stats-fr.json`). Settings are in `config.toml`, described in [Configuration](configuration.md#configtoml).

## Can I reset my stats?

`keybr-tui --reset` deletes the stats file of your practice language only, keeping your config and the other languages' stats, and then opens the app on a fresh profile. To clear the stats of every language, delete the `stats*.json` files in the folder `keybr-tui --data-dir` prints.

## What terminal emulators are supported?

Any terminal with the 16 standard ANSI colors and a monospace font works: Alacritty, Kitty, WezTerm, foot, iTerm2, Windows Terminal, GNOME Terminal and others.

## Why does keybr-tui keep showing me the same letter?

The letter keybr-tui keeps showing is your focus key: the active letter furthest below your target speed. keybr-tui puts it in every word until its speed reaches the target, then moves on to the next weakest letter.

## Can I drill specific letter combinations, like word endings?

Yes, in English and English (UK): the Focus pattern row in Settings pins one of 16 drills, such as `cr` or `-tion`. A drill appears there once your unlocked letters can spell enough words for it; see [focus_pattern drills](configuration.md#focus_pattern-drills).

## How do I change my target WPM?

Open Settings and change the Target WPM row with `Left`/`Right`. You can also set `target_wpm` in [config.toml](configuration.md#configtoml) or pass `--target-wpm <N>` for one run.

## What does "forgive mistakes" vs "stop on error" do?

In Forgive Mistakes mode (`forgive-mistakes` in config.toml), a wrong key is marked and the cursor still advances, so a typo never blocks the line. In Stop On Error mode (`stop-on-error`), the cursor waits until you type the right character, which trains accuracy more strictly.

## Can I use a Dvorak or Colemak layout?

keybr-tui reads whatever character your OS sends, so any keyboard layout works. The unlock order comes from the practice language's letter frequencies, as on keybr.com by default. When importing, `--layout` picks which layout family's sessions to replay.

## Does it support languages other than English?

Yes: keybr-tui teaches 27 languages, listed with their codes under [Languages](../README.md#languages). Typing letters outside `a` to `z` needs that language's keyboard layout in your OS and a UTF-8 terminal. The heatmap gives every letter its own tile: Turkish `İ` and `I` are separate keys, Greek tiles keep their accents, and German `ß` shows as `ẞ`.

## What does "taria layer disabled" mean?

"taria layer disabled" means keybr-tui could not open its taria socket, which lets AI agents read the screen and drive the app; everything else works as usual. On Linux and macOS an agent connects with `taria-mcp --app keybr-tui`, which finds the socket at `$XDG_RUNTIME_DIR/taria/keybr-tui.sock` (or under a `taria-<user>` folder in the temp directory). If that path is not usable, set `TARIA_SOCK` to another one for both programs.

## How can I contribute?

Read [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the repository root for the development setup, coding conventions, and pull-request workflow. Bug reports and small fixes are welcome via [GitHub issues](https://github.com/y0sif/keybr-tui/issues).
