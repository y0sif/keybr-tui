# keybr-tui README demo. Regenerate from the repo root, see demo/README.md:
#
#   cargo build --release
#   demo/render.sh
#
# render.sh runs this tape (which writes raw frames to demo/out/frames/) and
# encodes assets/demo.gif and demo/out/demo.mp4 with ffmpeg.
#
# The lesson text is random, so the typing itself comes from demo/drive.py,
# which reads the lesson over the app's taria socket and types it at a human
# pace (one corrected typo). VHS drives everything else.

Output demo/out/frames/

Require uv

Set Shell bash
Set Theme "Catppuccin Mocha"
Set FontSize 19
Set Width 1200
Set Height 680
Set Padding 20
Set WindowBar Colorful
Set TypingSpeed 75ms
Set Framerate 30
Set CursorBlink false

# Setup, not recorded: a throwaway profile so nothing touches real config or
# stats, a config with a few more letters unlocked and a short lesson, and the
# typing driver waiting in the background for the app's socket.
Hide
Type "export DEMO_HOME=$(mktemp -d)"
Enter
Type "export XDG_CONFIG_HOME=$DEMO_HOME/config XDG_DATA_HOME=$DEMO_HOME/data TARIA_SOCK=$DEMO_HOME/taria.sock"
Enter
Type "mkdir -p $XDG_CONFIG_HOME/keybr-tui && printf 'alphabet_size = 0.35\nfragment_length = 52\n' > $XDG_CONFIG_HOME/keybr-tui/config.toml"
Enter
Type "keybr-tui() { $PWD/target/release/keybr-tui 2>/dev/null; }"
Enter
Type "(uv run --quiet demo/drive.py >/dev/null 2>&1 &)"
Enter
Type "clear"
Enter
Sleep 500ms
Show

# Launch.
Sleep 600ms
Type "keybr-tui"
Sleep 400ms
Enter
Wait+Screen /Start Practice/
Sleep 1.4s

# Start a lesson; drive.py types it.
Enter
Wait+Screen@30s /Lesson 2/

# The finished lesson's speed and accuracy are on the dashboard; then a look
# at the per-key progress screen.
Sleep 2.2s
Escape
Sleep 700ms
Down
Sleep 500ms
Enter
Sleep 2.8s

# Back out and quit.
Escape
Sleep 600ms
Type "q"
Sleep 800ms

# Clean up the throwaway profile off camera.
Hide
Type "rm -rf $DEMO_HOME"
Enter
