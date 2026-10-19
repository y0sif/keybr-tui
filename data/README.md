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
