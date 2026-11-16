# README demo

`assets/demo.gif` is recorded with [VHS](https://github.com/charmbracelet/vhs)
and encoded with ffmpeg. To regenerate it, from the repo root:

```sh
cargo build --release
demo/render.sh
```

It needs `vhs`, `ttyd`, `ffmpeg` and `uv`, and writes `assets/demo.gif` and
`demo/out/demo.mp4` (`demo/out/` is git-ignored). `SKIP_RECORD=1 demo/render.sh`
re-encodes the last recording without recording again. The comments in
`demo.tape`, `drive.py` and `render.sh` explain how the recording works.
