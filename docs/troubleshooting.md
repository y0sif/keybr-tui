# Troubleshooting

Each entry gives the symptom, its cause and the fix. For anything else, see [Reporting bugs](#reporting-bugs).

## Text rendering looks broken / characters misaligned

**Symptom**: The typing line wraps oddly, the cursor sits on the wrong column, or characters overlap.

**Cause**: The terminal is rendering with a proportional or non-Unicode font, or the window is too small for the layout.

**Fix**: Switch to a monospace font (a Nerd Font or any standard mono works), and resize the terminal to at least 80 columns by 24 rows.

## Colors look wrong or muted

**Symptom**: The focus key, error highlights, or accent colors render as plain gray or look washed out.

**Cause**: keybr-tui draws only with the 16 standard ANSI colors, so your terminal's color theme decides how they look. A theme that maps several of them to similar shades hides the difference.

**Fix**: Pick a theme whose red, green, yellow, white and bright black (dark gray) are clearly distinct, along with their bright variants.

## Keypress feels laggy

**Symptom**: There is a visible delay between pressing a key and the character appearing, or WPM readings seem lower than they should be.

**Cause**: Input buffering between your keyboard and keybr-tui, most commonly in tmux or over an SSH session with a slow link.

**Fix**: Run keybr-tui directly in the terminal emulator (no tmux, no screen, no SSH) and compare. If the delay disappears, the layer in between is the cause.

## Stats reset every run

**Symptom**: Per-key speeds and session history are empty every time you launch the app.

**Cause**: keybr-tui could not save your stats, or it is loading another language's stats.

**Fix**:

- Read the terminal after you quit. A failed save prints `Error: could not save stats to <path>: <reason>` (or `config`) and keybr-tui exits with status 1. Make sure your user can write to the folder `keybr-tui --data-dir` prints.
- Each language keeps its own stats: check the Language row in Settings.
- A `--reset` in an alias or a script starts fresh on every run.

## "Moved it to ... .corrupt-..." warning

**Symptom**: After quitting you see `Warning: malformed stats at <path>: ... Moved it to <path>.corrupt-<time>. Starting fresh.` For the config file it ends in `Using defaults.`, and a file that cannot be read at all says `could not read` instead of `malformed`. Your stats or settings are back to defaults.

**Cause**: The file existed but could not be read or parsed when keybr-tui loaded it. Saves never leave a half-written file, so the usual causes are a hand edit or a sync tool.

**Fix**: keybr-tui never overwrites the broken file: it stays next to the original with the `.corrupt-<time>` suffix (Unix seconds). Quit keybr-tui, then copy the fixed `.corrupt-` file, or a backup, over the file the warning names. [Configuration](configuration.md#configtoml) lists every valid key and value.

## Can't find the config file

**Symptom**: You edited `config.toml` but settings did not change, or you cannot find the file at all.

**Cause**: The file does not exist until keybr-tui first saves a setting, a running keybr-tui rewrites it on its next save, and an invalid file is moved aside. [Configuration](configuration.md#configtoml) gives its path on each OS.

**Fix**: Change any value on the Settings screen to create the file, quit, then edit it.

## Crash on startup

**Symptom**: keybr-tui exits immediately with an error or panic message before the UI appears.

**Cause**: keybr-tui needs an interactive terminal, so it stops with an error when none is attached, as in a cron job or a service. Anything else is a bug: a broken stats or config file never stops it (see the `.corrupt-` warning above).

**Fix**: Run it in a terminal window. If it still exits, capture the message and attach it to a bug report:

```bash
RUST_BACKTRACE=1 keybr-tui 2> crash.log
```

## Reporting bugs

Open an issue with the [bug report template](https://github.com/y0sif/keybr-tui/issues/new/choose), which lists what to include.
