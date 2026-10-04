#!/usr/bin/env python3
"""Type one keybr-tui lesson for the README demo recording.

keybr-tui generates each lesson from a time-seeded RNG, so a VHS tape cannot
know the text in advance. The app does publish its screen over taria (an
agent-accessibility socket), including the lesson text as the `target-text`
node, and it accepts typing over the same socket, scored exactly like a
keypress. This script connects to that socket, waits for the typing screen,
reads the lesson, and types it at a human pace with one corrected typo.

Standard library only. Run it in the background before launching the app,
with TARIA_SOCK pointing at the same path the app will bind:

    uv run demo/drive.py &
"""

import json
import os
import random
import socket
import sys
import time

SOCK = os.environ.get("TARIA_SOCK")
if not SOCK:
    sys.exit("drive.py: TARIA_SOCK is not set")

# Fixed seed: the pacing (and where the typo lands) is the same every run.
rng = random.Random(7)

# Keys next to each other on a QWERTY row, for a plausible slip.
NEIGHBOURS = {
    "a": "s", "s": "d", "d": "f", "e": "r", "r": "t", "t": "y", "i": "o",
    "o": "p", "n": "m", "l": "k", "h": "j", "u": "y", "c": "v", "g": "h",
}


def connect():
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.connect(SOCK)
            return s
        except OSError:
            time.sleep(0.1)
    sys.exit("drive.py: app socket never appeared")


class Link:
    def __init__(self, sock):
        self.sock = sock
        self.buf = b""
        self.nodes = {}
        self.next_id = 1
        sock.setblocking(False)

    def pump(self):
        """Read whatever lines have arrived and keep the latest snapshot."""
        while True:
            try:
                chunk = self.sock.recv(65536)
            except BlockingIOError:
                return
            if not chunk:
                sys.exit(0)  # app quit
            self.buf += chunk
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                if not line.strip():
                    continue
                msg = json.loads(line)
                if msg.get("type") == "snapshot":
                    self.nodes = {}
                    self._index(msg["root"])

    def _index(self, node):
        self.nodes[node["id"]] = node
        for child in node.get("children", []):
            self._index(child)

    def value(self, node_id):
        node = self.nodes.get(node_id)
        return node.get("value") if node else None

    def send(self, payload):
        msg = {"type": "input", "id": self.next_id, "input": payload}
        self.next_id += 1
        self.sock.sendall((json.dumps(msg) + "\n").encode())

    def char(self, c):
        self.send({"kind": "text", "text": c})

    def key(self, k):
        self.send({"kind": "key", "key": k})


def pause(seconds, link):
    end = time.time() + seconds
    while time.time() < end:
        link.pump()
        time.sleep(0.01)


def main():
    link = Link(connect())

    # Wait for the typing screen with an untouched lesson.
    while True:
        link.pump()
        typed = link.value("typed")
        text = link.value("target-text")
        if text and typed and typed.startswith("0 of"):
            break
        time.sleep(0.02)

    pause(0.9, link)  # a beat to read the line before starting

    # One typo, in the middle of a word roughly a third of the way in.
    typo_at = None
    for i in range(len(text) // 3, len(text) - 2):
        if text[i] in NEIGHBOURS and text[i - 1] != " " and text[i + 1] != " ":
            typo_at = i
            break

    for i, c in enumerate(text):
        if i == typo_at:
            link.char(NEIGHBOURS[c])
            pause(0.32, link)  # notice it
            link.key("backspace")
            pause(0.22, link)
        link.char(c)
        delay = rng.gauss(0.135, 0.03)
        if c == " ":
            delay += rng.uniform(0.02, 0.09)
        pause(max(0.07, delay), link)

    pause(0.5, link)


if __name__ == "__main__":
    main()
