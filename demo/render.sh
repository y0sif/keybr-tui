#!/usr/bin/env bash
# Record demo/demo.tape and encode assets/demo.gif and demo/out/demo.mp4.
# Run from the repo root after `cargo build --release`. See demo/README.md.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=demo/out
FPS=30       # recording / mp4 frame rate (matches `Set Framerate` in the tape)
GIF_FPS=15   # the GIF is decimated to keep it small

mkdir -p assets

# VHS 0.12.0 never runs ffmpeg for Output .gif/.mp4 (its render context is
# already cancelled), so the tape only writes raw PNG frames and this script
# encodes them. VHS moves its temp dir into place with a rename, so the temp
# dir has to be on the same filesystem as the repo.
# SKIP_RECORD=1 re-encodes the frames from the last recording.
if [ -z "${SKIP_RECORD:-}" ]; then
  rm -rf "$OUT/frames" "$OUT/tmp"
  mkdir -p "$OUT/tmp"
  TMPDIR="$PWD/$OUT/tmp" vhs -q demo/demo.tape
fi

# Frame chrome VHS would normally add: Catppuccin Mocha base background,
# 34px padding, and a 46px window bar with the three colored dots.
IFS=x read -r W H < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
  -of csv=p=0:s=x "$OUT/frames/frame-text-00001.png")
PAD=34; BAR=46
CW=$((W + 2 * PAD)); CH=$((H + BAR + PAD))
dot() { echo "lte(hypot(X-$1,Y-23),7)"; }
ffmpeg -loglevel error -y -f lavfi -i "color=c=0x1e1e2e:s=${CW}x${CH}" -frames:v 1 \
  -vf "format=rgb24,geq=r='if($(dot 30),255,if($(dot 54),255,if($(dot 78),24,30)))':g='if($(dot 30),95,if($(dot 54),189,if($(dot 78),193,30)))':b='if($(dot 30),88,if($(dot 54),46,if($(dot 78),50,46)))'" \
  "$OUT/bg.png"

COMPOSE="[0][1]overlay[t];[2][t]overlay=${PAD}:${BAR}"
INPUTS=(-framerate "$FPS" -i "$OUT/frames/frame-text-%05d.png"
        -framerate "$FPS" -i "$OUT/frames/frame-cursor-%05d.png"
        -loop 1 -i "$OUT/bg.png")

ffmpeg -loglevel error -y "${INPUTS[@]}" -filter_complex "${COMPOSE}:shortest=1,format=yuv420p" \
  -c:v libx264 -crf 18 -preset slow -movflags +faststart "$OUT/demo.mp4"

ffmpeg -loglevel error -y "${INPUTS[@]}" -filter_complex \
  "${COMPOSE}:shortest=1,fps=${GIF_FPS},split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
  assets/demo.gif

rm -rf "$OUT/tmp"
ls -la assets/demo.gif "$OUT/demo.mp4"
