# Comparison with Alternatives

keybr-tui occupies the intersection of *adaptive* and *terminal-native*. Here is how it relates to common alternatives.

| Tool                    | Type   | Adaptive?                            | Persistent stats?   | Notes                                                                       |
| ----------------------- | ------ | ------------------------------------ | ------------------- | --------------------------------------------------------------------------- |
| keybr.com               | Web    | Yes (origin algorithm)               | Yes (account)       | Reference implementation; requires browser + internet.                      |
| keybr-tui               | TUI    | Yes (Markov phonetic + confidence)   | Yes (local files)   | This project.                                                               |
| tt                      | TUI    | No                                   | Limited             | Fast/minimalist; uses a wordlist, no per-key confidence scheduling.         |
| typer / toipe           | TUI    | No                                   | Limited             | Wordlist-based practice; no adaptive letter unlocking.                      |
| monkeytype              | Web    | Partial                              | Yes                 | Strong UI; not algorithm-adaptive in the keybr sense.                       |
| typing.com / typingclub | Web    | Lesson-based                         | Yes                 | Curriculum, not statistical adaptation.                                     |

## When to choose keybr-tui

- You live in a terminal and want a typing trainer that does too.
- You want adaptive, per-key practice without creating an account or opening a browser.
- You want your typing data stored locally, on your own machine, in plain files.
- You prefer minimalist, terminal-native UI over animated web frontends.
- You want the keybr.com adaptive algorithm available offline.

## When to choose something else

- You want a polished graphical UI with animations and rich theming — pick monkeytype or keybr.com.
- You want online leaderboards, social features, or cross-device sync — pick a web-based tool.
- You want a structured curriculum that teaches typing from scratch with guided lessons — pick typing.com or typingclub.
