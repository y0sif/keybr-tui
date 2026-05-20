# Troubleshooting

If you hit a problem not listed here, please open an issue at https://github.com/y0sif/keybr-tui/issues.

## Text rendering looks broken / characters misaligned

**Symptom**: The typing line wraps oddly, the cursor sits on the wrong column, or characters overlap.

**Cause**: The terminal is rendering with a proportional or non-Unicode font, or the window is too narrow for the layout to compute correctly.

**Fix**: Switch to a true monospace font (a Nerd Font or any standard mono works), and resize the terminal to at least 80 columns by 24 rows.

## Colors look wrong or muted

**Symptom**: The focus key, error highlights, or accent colors render as plain gray or look washed out.

**Cause**: Terminal color profile mismatch — the emulator is reporting a palette keybr-tui's `Style` values do not map cleanly onto.

**Fix**: Check `$TERM` and `$COLORTERM` in your shell. For most modern emulators you want `TERM=xterm-256color` and `COLORTERM=truecolor`. Restart the emulator after changing them.

```bash
echo $TERM
echo $COLORTERM
```

## Keypress feels laggy

**Symptom**: There is a visible delay between pressing a key and the character appearing, or WPM readings seem lower than they should be.

**Cause**: Input buffering somewhere in the stack — most commonly tmux passthrough or an SSH session with a slow link.

**Fix**: Run keybr-tui directly in the terminal emulator (no tmux, no screen, no SSH) and compare. If latency disappears, the buffering layer is the multiplexer, not keybr-tui.

## Stats reset every run

**Symptom**: Per-key confidence and session history are empty every time you launch the app.

**Cause**: The data directory does not exist or is not writable, so keybr-tui falls back to an in-memory profile.

**Fix**: Locate the resolved path and check permissions on the parent directory:

```bash
keybr-tui --data-dir
ls -ld ~/.local/share/keybr-tui/
```

Make sure the directory exists and is writable by your user.

## Can't find the config file

**Symptom**: You edited `config.toml` but settings did not change, or you cannot find the file at all.

**Cause**: The config file is not auto-created on first run. keybr-tui ships with sensible defaults and only reads the file if you put one in place.

**Fix**: Create it manually at `~/.config/keybr-tui/config.toml` (or the platform equivalent). See the configuration section in the README for the available keys.

```toml
target_wpm = 50
error_mode = "move-on"
```

## Crash on startup

**Symptom**: keybr-tui exits immediately with an error or panic message before the UI appears.

**Cause**: Could be a missing data directory, an incompatible terminal, a corrupted stats file, or a genuine bug.

**Fix**: Capture a backtrace and attach it to a GitHub issue so the cause can be identified:

```bash
RUST_BACKTRACE=1 keybr-tui 2>&1 | tee crash.log
```

## Reporting bugs

When opening an issue, please include:

- Operating system and version (e.g. Arch Linux, macOS 14, Windows 11).
- Terminal emulator and version (e.g. Alacritty 0.13, WezTerm 20240203).
- keybr-tui version from `keybr-tui --version`.
- Reproduction steps — what you ran, what you expected, what you saw.
- Relevant log output, ideally captured with `RUST_BACKTRACE=1`.
