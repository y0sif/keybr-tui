# keybr.com data

This directory holds the phonetic model and the word list of each of keybr-tui's practice
languages, copied from the [keybr.com source tree](https://github.com/aradzie/keybr.com) by
aradzie. keybr.com is licensed under the
[GNU AGPLv3](https://github.com/aradzie/keybr.com/blob/05a37bc5265f65ff538c59c613db29442f345d51/LICENSE),
and keybr-tui uses and redistributes this data under the same terms, as an AGPL-3.0-only
program. The keyboard layout table in `src/engine/layout.rs` is keybr.com data under the same
license. The models are exact copies; the word lists are normalised by the rules below.

## Sources

Every file comes from keybr.com at commit
[`05a37bc`](https://github.com/aradzie/keybr.com/tree/05a37bc5265f65ff538c59c613db29442f345d51).
A file's raw URL is
`https://raw.githubusercontent.com/aradzie/keybr.com/05a37bc5265f65ff538c59c613db29442f345d51/<upstream path>`,
and each model's sha256 (first 12 digits) matches its upstream file.

| File | Upstream path | Commit | sha256 |
|---|---|---|---|
| `model-en.data` | `packages/keybr-phonetic-model/assets/model-en.data` | `05a37bc` | `4a389241e2f0` |
| `model-en-gb.data` | `packages/keybr-phonetic-model/assets/model-en-GB.data` | `05a37bc` | `f26f01564c72` |
| `model-be.data` | `packages/keybr-phonetic-model/assets/model-be.data` | `05a37bc` | `bcb22b0889f6` |
| `model-cs.data` | `packages/keybr-phonetic-model/assets/model-cs.data` | `05a37bc` | `2fb0dbee900e` |
| `model-da.data` | `packages/keybr-phonetic-model/assets/model-da.data` | `05a37bc` | `de427293818a` |
| `model-de.data` | `packages/keybr-phonetic-model/assets/model-de.data` | `05a37bc` | `f0009a7a68c7` |
| `model-el.data` | `packages/keybr-phonetic-model/assets/model-el.data` | `05a37bc` | `4244cb771baf` |
| `model-es.data` | `packages/keybr-phonetic-model/assets/model-es.data` | `05a37bc` | `3c5a3c861ddb` |
| `model-et.data` | `packages/keybr-phonetic-model/assets/model-et.data` | `05a37bc` | `dbc042918e2a` |
| `model-fi.data` | `packages/keybr-phonetic-model/assets/model-fi.data` | `05a37bc` | `ec41a08526a5` |
| `model-fr.data` | `packages/keybr-phonetic-model/assets/model-fr.data` | `05a37bc` | `98a4974fc2da` |
| `model-hr.data` | `packages/keybr-phonetic-model/assets/model-hr.data` | `05a37bc` | `42c0becafe28` |
| `model-hu.data` | `packages/keybr-phonetic-model/assets/model-hu.data` | `05a37bc` | `6974a0f80a1b` |
| `model-hy.data` | `packages/keybr-phonetic-model/assets/model-hy.data` | `05a37bc` | `448412be3cd1` |
| `model-it.data` | `packages/keybr-phonetic-model/assets/model-it.data` | `05a37bc` | `67386cfe88e5` |
| `model-lt.data` | `packages/keybr-phonetic-model/assets/model-lt.data` | `05a37bc` | `0a6ccfe11311` |
| `model-lv.data` | `packages/keybr-phonetic-model/assets/model-lv.data` | `05a37bc` | `36d587dd45c5` |
| `model-nb.data` | `packages/keybr-phonetic-model/assets/model-nb.data` | `05a37bc` | `7230a7bb2794` |
| `model-nl.data` | `packages/keybr-phonetic-model/assets/model-nl.data` | `05a37bc` | `5c18b308744e` |
| `model-pl.data` | `packages/keybr-phonetic-model/assets/model-pl.data` | `05a37bc` | `946ad82d7f93` |
| `model-pt.data` | `packages/keybr-phonetic-model/assets/model-pt.data` | `05a37bc` | `fe87159376b1` |
| `model-ro.data` | `packages/keybr-phonetic-model/assets/model-ro.data` | `05a37bc` | `e133bcc3ff64` |
| `model-ru.data` | `packages/keybr-phonetic-model/assets/model-ru.data` | `05a37bc` | `4a00f67fdd15` |
| `model-sl.data` | `packages/keybr-phonetic-model/assets/model-sl.data` | `05a37bc` | `9f13097324bd` |
| `model-sv.data` | `packages/keybr-phonetic-model/assets/model-sv.data` | `05a37bc` | `f686f78ffdef` |
| `model-tr.data` | `packages/keybr-phonetic-model/assets/model-tr.data` | `05a37bc` | `fba23f6fbf8b` |
| `model-uk.data` | `packages/keybr-phonetic-model/assets/model-uk.data` | `05a37bc` | `4f51c18825ef` |
| `wordlist-<code>.txt` | `packages/keybr-content-words/lib/data/words-<id>.json` | `05a37bc` | normalised |
| `LAYOUTS` in `src/engine/layout.rs` | `packages/keybr-keyboard/lib/layout.ts` | `05a37bc` | transcribed |

`<code>` is the language's config code and `<id>` its keybr.com id. They are the same except
for British English, `en-gb` here and `en-GB` upstream. The `src/engine/layout.rs` module
comment says how its table is transcribed.

## Word list normalisation

Each `wordlist-<code>.txt` is made from keybr.com's `words-<id>.json` by one set of rules.

- Keep upstream order.
- Keep only words made entirely of the language's lowercase letters, the alphabet in its
  model's header. A capitalised word is dropped, not lowercased. English letters are ASCII
  `a` to `z`, so an English word with an apostrophe or an accent is dropped.
- Keep words of 2 to 12 characters (characters, not bytes). keybr.com keeps the longer ones.
- Drop duplicates, keeping the first.
- Write one word per line in UTF-8, ending with a newline.

## Adding a language

A new language needs two files here and a row for its model in the table above.

- `model-<code>.data`: keybr.com's `model-<id>.data`, copied unchanged.
- `wordlist-<code>.txt`: keybr.com's `words-<id>.json`, normalised by the rules above.

Take both from one keybr.com commit and record it in the table. The code side is in
[AGENTS.md](../AGENTS.md).
