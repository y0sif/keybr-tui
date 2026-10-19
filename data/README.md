# Phonetic transition models

This directory contains binary phonetic Markov-chain models used by the
text generator in `src/engine/`.

## `model-en.data`

An order-4 phonetic transition table for English. Each segment is a
27-entry frequency distribution (space + a–z) conditioned on the
previous three characters, which gives the generator enough context
to emit shapes that read as plausible English (e.g. "ation", "ence",
"tion") rather than the bigram-boundary gibberish ("ata ete tat") that
an order-2 model produces.

### Source

Copied verbatim from the [keybr.com source tree](https://github.com/aradzie/keybr.com),
at `packages/keybr-phonetic-model/assets/model-en.data`.

Direct file URL (master branch):
<https://raw.githubusercontent.com/aradzie/keybr.com/master/packages/keybr-phonetic-model/assets/model-en.data>

### Format

Binary. Multi-byte fields are big-endian, as keybr.com's reader
(`packages/keybr-binary/lib/io.ts`) reads them:

| Bytes        | Field                                                           |
| ------------ | --------------------------------------------------------------- |
| `0..9`       | ASCII signature `keybr.com`                                     |
| `9`          | Markov order (`4`)                                              |
| `10`         | Alphabet size (`27`)                                            |
| `11..65`     | 27 × `u16 BE` code points: `0x0020`, `'a'`..`'z'`               |
| `65..end`    | Sparse segments — one per `(order - 1)`-character history       |

Each segment is encoded as:

```
[1 byte] N — number of non-zero entries
[N pairs] (alphabet_index u8, frequency u8)
```

Frequencies in each non-empty segment sum to 255. Impossible histories
(those that never occurred in the training corpus) are encoded as
zero-length segments. See
`packages/keybr-phonetic-model/lib/transitiontable.ts` upstream for the
canonical encoder/decoder.

The letters and their unlock order are read from the file itself, for
every language: the letters from the alphabet header, the unlock order as
keybr.com's `Letter.frequencyOrder` (each letter's frequencies summed over
all segments, highest first). See `ModelLetters` in
`src/engine/transition.rs`.

### License

The keybr.com source code is licensed under the [GNU AGPLv3](https://github.com/aradzie/keybr.com/blob/master/LICENSE.md).
The phonetic model file is distributed with that source and is used
here under the same terms. Redistribution of this binary asset within
the keybr-tui repository preserves attribution to the original
authors at <https://github.com/aradzie/keybr.com>.

## `model-tr.data`

The Turkish model, same format: order 4, alphabet size 30 (space followed
by `abcçdefgğhıijklmnoöprsştuüvyz`, code points stored big-endian like the
English file). Copied verbatim from
`packages/keybr-phonetic-model/assets/model-tr.data` in the keybr.com
source tree. Its unlock order (`a e i n ı r` first) is read from it as
above, and a test in `src/engine/language.rs` pins it.

## `model-de.data`

The German model, same format: order 4, alphabet size 31 (space followed
by `aäbcdefghijklmnoöpqrstuüvwxyzß`, code points stored big-endian).
Copied verbatim from
`packages/keybr-phonetic-model/assets/model-de.data` in the keybr.com
source tree.

Direct file URL (master branch):
<https://raw.githubusercontent.com/aradzie/keybr.com/master/packages/keybr-phonetic-model/assets/model-de.data>

Its unlock order, read from it as above, is
`e n t i r l h a s c g u m o f d k b p z ü ä ß w ö v y q x j`; a test in
`src/engine/language.rs` pins it. Same licence as the English model.

## `wordlist-en.txt`

A list of real English words used by the natural-words blend in
`src/engine/dictionary.rs`. When the focused letter and active letter
filter permit, the generator returns one of these words instead of a
phonetic Markov sample, producing immediately recognisable practice
text.

### Source

Derived from the keybr.com word list at
`packages/keybr-content-words/lib/data/words-en.json`
(10,000 most common English words, English fiction corpus).

Direct file URL (master branch):
<https://raw.githubusercontent.com/aradzie/keybr.com/master/packages/keybr-content-words/lib/data/words-en.json>

### Normalisation

- JSON string array → one word per line
- Lowercased, ASCII a–z only (any word containing apostrophes, accents,
  or other characters dropped)
- Length filtered to 2..=12 characters
- Duplicates removed

Result: 9,906 lines, ~76 KB.

### License

Same upstream licence as the phonetic model — keybr.com is GNU AGPLv3.
Attribution is preserved here through this file and through the
embedded source comment in `src/engine/dictionary.rs`.

## `wordlist-tr.txt`

Real Turkish words for the natural-words blend.

### Source

keybr.com's `packages/keybr-content-words/lib/data/words-tr.json`
(8,886 words).

### Normalisation

- JSON string array → one word per line, UTF-8
- Lowercased with Turkish rules (`I` → `ı`, `İ` → `i`), only words made of
  the 29 Turkish letters kept
- Length filtered to 2..=12 characters (characters, not bytes)
- Duplicates removed

Result: 8,752 lines. Of the 134 dropped words, 132 are longer than 12
characters and 2 are single letters (`u`, `ü`).

### License

Same upstream licence as above — keybr.com is GNU AGPLv3.

## `wordlist-de.txt`

Real German words for the natural-words blend.

### Source

keybr.com's `packages/keybr-content-words/lib/data/words-de.json`
(10,000 words).

Direct file URL (master branch):
<https://raw.githubusercontent.com/aradzie/keybr.com/master/packages/keybr-content-words/lib/data/words-de.json>

### Normalisation

- JSON string array → one word per line, UTF-8, upstream order kept
- Only words made of the 30 lowercase German letters
  (`aäbcdefghijklmnoöpqrstuüvwxyzß`) kept. Capitalised words are dropped,
  not lowercased: keybr.com's lessons only offer words made of the
  lesson's letters (`filterWordList` in
  `packages/keybr-lesson/lib/dictionary.ts`), which are lowercase, so it
  never shows them either
- Length filtered to 2..=12 characters (characters, not bytes)
- No duplicates to remove

Result: 5,559 lines. Of the 4,441 dropped words, 4,242 are capitalised and
199 are longer than 12 characters.

### License

Same upstream licence as above: keybr.com is GNU AGPLv3.

## The other languages

keybr-tui teaches 24 more languages, each from two files named by its
config code: `model-<code>.data` and `wordlist-<code>.txt`. Both come from
the keybr.com source tree at commit `05a37bc`, by the same rules for every
language:

- The model is `packages/keybr-phonetic-model/assets/model-<id>.data`,
  copied verbatim (same format as above). The alphabet in each header,
  after its leading space, is exactly keybr.com's `Language.alphabet` for
  that language (`packages/keybr-keyboard/lib/language.ts`), in order. The
  unlock order is read from the model as above; tests in
  `src/engine/language.rs` pin the letters and an unlock order derived
  from each file outside this crate.
- The word list is `packages/keybr-content-words/lib/data/words-<id>.json`
  under the German rules: only words made entirely of the language's
  lowercase letters (a capitalised word is dropped, not lowercased, and so
  is a word with any other character), 2..=12 characters (not bytes),
  duplicates dropped keeping the first, upstream order, one word per line.

British English is keybr.com's `en-GB`; its files use the config code,
`model-en-gb.data` and `wordlist-en-gb.txt`.

| Code    | keybr id | Letters | Model sha256   | Words kept | Dropped (capitalised / other / length) |
| ------- | -------- | ------: | -------------- | ---------: | -------------------------------------- |
| `en-gb` | `en-GB`  |      26 | `f26f01564c72` |      9,211 | 0 / 0 / 224                            |
| `be`    | `be`     |      31 | `bcb22b0889f6` |      9,962 | 0 / 0 / 38                             |
| `cs`    | `cs`     |      39 | `2fb0dbee900e` |      9,931 | 0 / 0 / 69                             |
| `da`    | `da`     |      29 | `de427293818a` |      9,676 | 0 / 0 / 324                            |
| `el`    | `el`     |      32 | `4244cb771baf` |      9,790 | 0 / 0 / 201                            |
| `es`    | `es`     |      30 | `3c5a3c861ddb` |      9,839 | 0 / 0 / 161                            |
| `et`    | `et`     |      25 | `dbc042918e2a` |      9,319 | 0 / 0 / 361                            |
| `fi`    | `fi`     |      23 | `ec41a08526a5` |      9,724 | 0 / 0 / 276                            |
| `fr`    | `fr`     |      27 | `98a4974fc2da` |      9,846 | 0 / 0 / 154                            |
| `hr`    | `hr`     |      27 | `42c0becafe28` |      9,874 | 0 / 0 / 126                            |
| `hu`    | `hu`     |      35 | `6974a0f80a1b` |      8,337 | 0 / 0 / 310                            |
| `hy`    | `hy`     |      39 | `448412be3cd1` |      8,471 | 0 / 0 / 1,529                          |
| `it`    | `it`     |      26 | `67386cfe88e5` |      9,819 | 0 / 0 / 181                            |
| `lt`    | `lt`     |      32 | `0a6ccfe11311` |      9,925 | 0 / 0 / 75                             |
| `lv`    | `lv`     |      33 | `36d587dd45c5` |      9,601 | 0 / 0 / 399                            |
| `nb`    | `nb`     |      26 | `7230a7bb2794` |      9,509 | 0 / 0 / 491                            |
| `nl`    | `nl`     |      26 | `5c18b308744e` |      9,720 | 0 / 0 / 280                            |
| `pl`    | `pl`     |      32 | `946ad82d7f93` |      9,879 | 0 / 0 / 121                            |
| `pt`    | `pt`     |      35 | `fe87159376b1` |      9,838 | 0 / 0 / 162                            |
| `ro`    | `ro`     |      27 | `e133bcc3ff64` |      9,917 | 0 / 0 / 83                             |
| `ru`    | `ru`     |      32 | `4a00f67fdd15` |      9,374 | 0 / 0 / 626                            |
| `sl`    | `sl`     |      25 | `9f13097324bd` |      2,746 | 0 / 0 / 19                             |
| `sv`    | `sv`     |      27 | `f686f78ffdef` |      9,759 | 0 / 0 / 241                            |
| `uk`    | `uk`     |      33 | `4f51c18825ef` |      9,806 | 0 / 0 / 194                            |

No upstream list has capitalised words, other characters or duplicates,
and none is in decomposed (NFD) Unicode, so every drop is for length:
words longer than 12 characters, and single letters. Slovenian's upstream
list is short, 2,765 words.

### Letter casing

The heatmap shows each letter as keybr.com's `Language.letterName` does,
through Rust's `to_uppercase` and the overrides in `SPECS`
(`src/engine/language.rs`), with two notes:

- Armenian `և` shows as `ԵՎ`, as on keybr.com (Rust would give `ԵՒ`). It
  is the one two-letter tile, padded to the usual three cells. A pin on it
  is written as `և`.
- Greek deliberately differs. keybr.com's Greek uppercase drops the tonos
  (`ά έ ί ό ύ ώ` show as `Α Ε Ι Ο Υ Ω`, though `ή` keeps it) and shows `ς`
  as `Σ`, so seven pairs of tiles look alike there. keybr-tui keeps Rust's
  accented capitals `Ά Έ Ή Ί Ό Ύ Ώ` and shows final sigma as `ς` itself,
  so every tile is distinct; a test holds every language to that.

### License

Same upstream licence as above: keybr.com is GNU AGPLv3.

## Layout table (`src/engine/layout.rs`)

Not a file in this directory, but data copied from keybr.com all the same.
`LAYOUTS` in `src/engine/layout.rs` is keybr.com's `Layout.ALL`: 98
layouts, each with its id, language and family, which `--import` uses to
pick a session's language and layout family. Taken from
`packages/keybr-keyboard/lib/layout.ts` (language ids from
`packages/keybr-keyboard/lib/language.ts`) at commit `05a37bc`:
<https://github.com/aradzie/keybr.com/blob/05a37bc5265f65ff538c59c613db29442f345d51/packages/keybr-keyboard/lib/layout.ts>

The module comment says how to regenerate it. Same upstream licence:
keybr.com is GNU AGPLv3.
