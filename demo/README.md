# README demo

`assets/demo.gif` is recorded with [VHS](https://github.com/charmbracelet/vhs)
and encoded with ffmpeg. To regenerate it, from the repo root:

```sh
cargo build --release
demo/render.sh
```

Needs `vhs` (tested with 0.12.0), `ttyd`, `ffmpeg` and `uv`. It writes
`assets/demo.gif` and `demo/out/demo.mp4` (`demo/out/` is git-ignored).
`SKIP_RECORD=1 demo/render.sh` re-encodes the last recording without
recording again.

## How it works

- `demo.tape` sets up a throwaway profile (`XDG_CONFIG_HOME`,
  `XDG_DATA_HOME` and `TARIA_SOCK` under a `mktemp -d` dir, deleted at the
  end), so your real config and stats are never touched. Its `config.toml`
  sets `alphabet_size = 0.35` (a few more letters than a brand new profile)
  and `fragment_length = 52` (a short lesson).
- Lesson text comes from a time-seeded generator, so a tape cannot know it in
  advance. `drive.py` connects to the app's taria socket, reads the lesson
  from the `target-text` node, and types it one character at a time at about
  80 WPM with one corrected typo. Agent text input is scored exactly like a
  keypress, so the stats on screen are real. Pacing is seeded, so only the
  words change between runs.
- VHS 0.12.0 never runs ffmpeg for `Output demo.gif` (its render context is
  already cancelled when it gets there), so the tape writes raw PNG frames and
  `render.sh` encodes them: it adds the background, padding and window bar,
  writes the mp4 at 30 fps (libx264, CRF 18), and writes the GIF at 15 fps
  with a 128-color palette (`palettegen stats_mode=diff`, `paletteuse
  dither=none diff_mode=rectangle`). The GIF comes out around 140 KB.
