# Frequently Asked Questions

## What is keybr-tui?

keybr-tui is a terminal-native typing trainer built in Rust with ratatui. It uses the adaptive Markov-chain text generation algorithm from keybr.com to schedule practice around the keys you are weakest on.

## How is keybr-tui different from keybr.com?

keybr-tui is a faithful port of the keybr.com adaptive algorithm to a terminal UI. It runs entirely offline, stores your stats on disk in plain files, and has no account, browser, or network requirement.

## Can I import my keybr.com data into keybr-tui?

Yes. On keybr.com, open your profile page and click **Download data** to get a
`typing-data.json` file with your full practice history, then run
`keybr-tui --import typing-data.json`. The importer replays your sessions through
the same per-key smoothing keybr.com uses, so your unlocked letters, per-key
speeds, and focus letter come out exactly as the website computed them. It takes
the sessions of your practice language and, since keybr.com keeps key stats per
layout family, only the family of your latest session; `--layout` picks another
family (`colemak`, `qwertz`) or `all` to merge them. See the "Bring your
keybr.com history" section of the README for the full details.

## Does keybr-tui work offline?

Yes. keybr-tui never makes a network request. Text is generated locally by the phonetic Markov model, and all metrics are written to local files.

## Where is my progress saved?

Per-key stats and session history live under your platform's XDG data directory, typically `~/.local/share/keybr-tui/` on Linux. You can confirm the resolved path by running `keybr-tui --data-dir`.

## Can I reset my stats?

Yes. `keybr-tui --reset` deletes the stats file of your practice language only, keeping your config and the other languages' stats, and then opens the app on a fresh profile. To clear the stats of every language, delete the `stats*.json` files in the data directory (`keybr-tui --data-dir` prints it).

## What terminal emulators are supported?

Any modern emulator that supports a true-color or 256-color ANSI palette and a monospace font: Alacritty, Kitty, WezTerm, foot, iTerm2, Windows Terminal, GNOME Terminal, and others. Crossterm handles the platform differences.

## Why does keybr-tui keep showing me the same letter?

That is the adaptive algorithm working as designed. keybr-tui identifies your lowest-confidence key — the "focus key" — and biases generated text toward it until your accuracy and speed on that key catch up to the rest.

## Can I drill specific letter combinations, like word endings?

Yes. Settings has a "Focus pattern" row, cycled with Left/Right like the other rows,
offering 16 presets: 8 bigram reaches (`cr`, `pl`, `br`, `tr`, `gr`, `fr`, `bl`, `cl`)
and 8 word endings (`-ing`, `-ed`, `-er`, `-ly`, `-tion`, `-ment`, `-able`, `-ous`). A
preset only shows up once your unlocked letters can spell enough real words to fill a
lesson, so the list starts empty and grows as you unlock more. A fresh profile reaches
none of them and the row reads "None yet"; the first drill arrives with your seventh
letter (`-ER` in English, `TR` in English (UK)), and all 16 are available by 21
letters. Picking a pattern clears any letter pin, since only one focus can be active
at a time, and drills always draw from the embedded word list, regardless of the
"natural words" setting. The presets are English bigrams and endings, so only English
and English (UK) offer them.

## How do I change my target WPM?

Edit `~/.config/keybr-tui/config.toml` and set the `target_wpm` field. The config file is not auto-created on first run; see the README's configuration section for a template.

## What does "forgive mistakes" vs "stop on error" do?

In `move-on` mode (forgive mistakes), an incorrect keystroke is recorded but the cursor still advances, so a typo does not block the rest of the line. In `stop-on-error` mode, the cursor refuses to move until you type the correct character, which trains accuracy more strictly.

## Can I use a Dvorak or Colemak layout?

Yes. Typing is layout-agnostic: keybr-tui reads whatever character your OS sends. The unlock order comes from the practice language's letter frequencies, not from the keyboard layout, as on keybr.com by default. When importing, `--layout` picks which layout family's sessions to replay.

## Does it support languages other than English?

Yes, 27 in all: Armenian, Belarusian, Croatian, Czech, Danish, Dutch, English, English (UK), Estonian, Finnish, French, German, Greek, Hungarian, Italian, Latvian, Lithuanian, Norwegian Bokmål, Polish, Portuguese, Romanian, Russian, Slovenian, Spanish, Swedish, Turkish and Ukrainian. Switch with the "Language" row in Settings (Left/Right), or run `keybr-tui --language <code>` (`fr`, or a name such as `french`); either way, later runs remember it. Each language uses keybr.com's phonetic model, word list and letter unlock order for that language, and keeps its own stats. The "Languages" section of the README lists the codes.

## How can I contribute?

Read [`CONTRIBUTING.md`](../CONTRIBUTING.md) at the repository root for the development setup, coding conventions, and pull-request workflow. Bug reports and small fixes are welcome via GitHub issues at https://github.com/y0sif/keybr-tui/issues.
