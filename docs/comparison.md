# Comparison with Alternatives

keybr-tui is an adaptive typing trainer that runs in the terminal. Here is how it compares with common alternatives.

| Tool | Type | Adaptive? | Saved stats? | Notes |
|---|---|---|---|---|
| keybr.com | Web | Yes (origin algorithm) | Yes (account) | Reference implementation; needs a browser and an internet connection. |
| keybr-tui | TUI | Yes (keybr.com's algorithm) | Yes (local files) | This project. |
| tt | TUI | No | No (`-csv` prints each result) | Fast and minimal; draws from a word list, no per-key scheduling. |
| typer / toipe | TUI | No | No (score after each test) | Word-list practice; no letter unlocking. |
| monkeytype | Web | No (can replay missed words) | Yes (account) | Polished UI; not adaptive in the keybr sense. |
| typing.com / typingclub | Web | Lesson-based | Yes (account) | A fixed curriculum, not statistical adaptation. |

## When to choose keybr-tui

- You live in a terminal and want a typing trainer that does too.
- You want adaptive, per-key practice without creating an account or opening a browser.
- You want your typing data stored locally, on your own machine, in plain files.
- You prefer a minimal terminal UI over animated web frontends.
- You want the keybr.com adaptive algorithm available offline.

## When to choose something else

- You want a polished graphical UI with animations and rich theming: pick monkeytype or keybr.com.
- You want online leaderboards, social features, or cross-device sync: pick a web-based tool.
- You want a structured curriculum that teaches typing from scratch with guided lessons: pick typing.com or typingclub.
