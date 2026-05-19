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

Binary, big-endian-free (all multi-byte fields are little-endian):

| Bytes        | Field                                                           |
| ------------ | --------------------------------------------------------------- |
| `0..9`       | ASCII signature `keybr.com`                                     |
| `9`          | Markov order (`4`)                                              |
| `10`         | Alphabet size (`27`)                                            |
| `11..65`     | 27 × `u16 LE` code points: `0x0020`, `'a'`..`'z'`               |
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

### License

The keybr.com source code is licensed under the [GNU AGPLv3](https://github.com/aradzie/keybr.com/blob/master/LICENSE.md).
The phonetic model file is distributed with that source and is used
here under the same terms. Redistribution of this binary asset within
the keybr-tui repository preserves attribution to the original
authors at <https://github.com/aradzie/keybr.com>.

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
