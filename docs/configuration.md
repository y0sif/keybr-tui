# Configuration

keybr-tui runs with no configuration. Change settings on the Settings screen; edit `config.toml` only for the options the screen does not show.

## Settings screen

Open Settings from the main menu. Every change is saved to `config.toml` at once.

| Row | Values | Default |
|---|---|---|
| Target WPM | 10 to 200, in steps of 5 | 35 |
| Error Mode | Forgive Mistakes or Stop On Error (`Enter` also switches it) | Forgive Mistakes |
| Fragment Length | 20 to 500 characters, in steps of 10 | 100 |
| Alphabet size | `+0 letters` up to the whole alphabet | `+0 letters` |
| Focus letter | Auto, or one of your unlocked letters | Auto |
| Focus pattern | Off, or a combination drill on offer | Off |
| Language | The [languages](../README.md#languages), each by its own name | English |

- **Target WPM** is the speed a letter must reach to count as learned, so a higher target unlocks new letters later.
- **Error Mode**: Forgive Mistakes marks a wrong key and moves on; Stop On Error holds the cursor until you type the right key.
- **Fragment Length** is the number of characters in one lesson.
- **Alphabet size** unlocks letters ahead of time, in unlock order, without earning them. Lowering it never locks a letter again.
- **Focus letter** pins the focus on one unlocked letter, so every word contains it. Auto lets keybr-tui pick your weakest letter.
- **Focus pattern** pins a combination drill: every word contains the bigram or ends with the ending. It reads `None yet` until a drill is on offer (see `focus_pattern` below).
- **Language** switches at once and loads that language's own stats.

The focus letter and the focus pattern share one pin: setting one clears the other.

## config.toml

keybr-tui reads `config.toml` at launch and writes it whenever a setting changes. Edit it while keybr-tui is closed: a save from a running keybr-tui rewrites the whole file from its current settings.

| OS | Path |
|---|---|
| Linux | `~/.config/keybr-tui/config.toml` (under `$XDG_CONFIG_HOME` when it is set) |
| macOS | `~/Library/Application Support/keybr-tui/config.toml` |
| Windows | `%APPDATA%\keybr-tui\config\config.toml` |

The file does not exist until the first save. Stats are not in this file: `keybr-tui --data-dir` prints their folder.

```toml
target_wpm = 50
error_mode = "stop-on-error"
daily_goal_minutes = 15
```

| Key | Type | Default | Values |
|---|---|---|---|
| `target_wpm` | integer | `35` | Target speed in words per minute |
| `error_mode` | string | `"forgive-mistakes"` | `"forgive-mistakes"` or `"stop-on-error"` |
| `fragment_length` | integer | `100` | Characters per lesson |
| `natural_words` | boolean | `true` | `true`: real words that fit your letters, with phonetic pseudo-words added when fewer than 15 fit. `false`: phonetic pseudo-words only |
| `daily_goal_minutes` | integer | `30` | Minutes of practice per day for the dashboard's daily goal; `0` hides it |
| `alphabet_size` | float | `0.0` | `0.0` to `1.0`: the share of the letters past the six starters to unlock ahead of time. Values outside the range are clamped |
| `focus_letter` | string | unset | One letter of the practice alphabet, in either case. A locked letter is ignored, and in English the next save removes it |
| `focus_pattern` | string | unset | One of the drills below. It wins over `focus_letter` |
| `language` | string | `"en"` | A code from [Languages](../README.md#languages), or a language name. An unknown one falls back to English with a warning |

Unknown keys are ignored. A value of the wrong type, or an `error_mode` other than the two above, makes the whole file unreadable: keybr-tui moves it aside and starts with defaults (see [Troubleshooting](troubleshooting.md#moved-it-to--corrupt--warning)).

### focus_pattern drills

English and English (UK) offer 16 combination drills. In the file they are lowercase; the Settings row shows them in capitals.

- Bigrams: `"cr"`, `"pl"`, `"br"`, `"tr"`, `"gr"`, `"fr"`, `"bl"`, `"cl"`
- Word endings: `"-ing"`, `"-ed"`, `"-er"`, `"-ly"`, `"-tion"`, `"-ment"`, `"-able"`, `"-ous"`

A drill is on offer once all its letters are unlocked and the word list has at least 8 words for it. A new profile has none; in English the first, `-ER`, arrives with the seventh letter. A pinned drill that is not on offer yet is ignored, and in English the next save removes it. Drills always use real words, whatever `natural_words` says.

### Command-line flags

`--target-wpm`, `--error-mode` and `--language` override the file for one run. `--language` is also saved to the file. The other two are saved only if that run saves the config: a change in Settings, leaving the Settings screen, or `Tab` while typing writes every current value.
