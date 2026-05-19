//! Order-4 Markov phonetic transition model.
//!
//! Ported from upstream keybr.com
//! (`packages/keybr-phonetic-model/lib/transitiontable.ts`).
//!
//! The embedded binary file `data/model-en.data` encodes a phonetic
//! transition table trained on English text. Compared to the previous
//! 27×27 bigram model, this order-4 chain conditions each character on
//! the previous three characters, which dramatically reduces gibberish
//! at filter boundaries (e.g. the bigram model produced "ata ete tat";
//! the order-4 model produces shapes like "ente", "ation", "tio").
//!
//! ## On-disk format
//!
//! ```text
//! [9 bytes] ASCII signature "keybr.com"
//! [1 byte]  order (= 4)
//! [1 byte]  alphabet size (= 27)
//! [size*2]  alphabet code points (u16 BE): 0x0020 (space), 'a'..'z'
//! [...]     sparse segment data, one segment per (order-1)-char history:
//!             [1 byte] segment length N (entries with non-zero frequency)
//!             [N*2]    pairs of (alphabet_index u8, frequency u8)
//! ```
//!
//! Frequencies in each segment sum to 255 (or 0 for impossible histories).
//!
//! At load time we decode the sparse representation into a dense
//! `Vec<u8>` of length `alphabet_size^order` (≈ 519 KiB for English),
//! flattened in row-major order so segment lookup is a single index
//! computation: `offset = ((h0 * size) + h1) * size + h2`.

/// Embedded order-4 English phonetic model from upstream keybr.com.
const MODEL_BYTES: &[u8] = include_bytes!("../../data/model-en.data");

/// ASCII signature at the start of every model file.
const SIGNATURE: &[u8] = b"keybr.com";

/// Alphabet size we expect for English: space + a..z.
const EXPECTED_ALPHABET_SIZE: usize = 27;

/// Markov chain order we expect from the upstream English model.
const EXPECTED_ORDER: usize = 4;

/// Decoded order-4 transition table.
///
/// Once loaded, lookup is O(1): a segment is a contiguous 27-byte slice
/// containing the next-character frequency distribution given the last
/// `order - 1` characters.
pub struct TransitionTable {
    /// Markov chain order (= 4 for the English model).
    order: usize,
    /// Number of distinct symbols (= 27: space + a..z).
    alphabet_size: usize,
    /// Densely decoded segment data: `alphabet_size.pow(order)` bytes.
    ///
    /// Indexing: for history `[h0, h1, ..., h_{order-2}]` the segment
    /// starts at offset `segment_index(history) * alphabet_size`.
    data: Vec<u8>,
}

impl TransitionTable {
    /// Decode the embedded English model.
    ///
    /// Panics with a descriptive message if the binary asset is missing
    /// or malformed — this is a build-time/asset failure and not
    /// something the runtime should attempt to recover from.
    pub fn from_embedded() -> Self {
        Self::from_bytes(MODEL_BYTES).expect("embedded model-en.data is malformed")
    }

    /// Decode a transition table from a raw byte slice.
    ///
    /// Returns an error string describing the first malformed field
    /// encountered, suitable for `expect()` in callers.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = 0usize;

        // --- Signature ---
        if bytes.len() < SIGNATURE.len() {
            return Err("file too short for signature".into());
        }
        if &bytes[..SIGNATURE.len()] != SIGNATURE {
            return Err("missing keybr.com signature".into());
        }
        cursor += SIGNATURE.len();

        // --- Chain header ---
        if bytes.len() < cursor + 2 {
            return Err("file too short for chain header".into());
        }
        let order = bytes[cursor] as usize;
        cursor += 1;
        let alphabet_size = bytes[cursor] as usize;
        cursor += 1;

        if order != EXPECTED_ORDER {
            return Err(format!("unsupported order {} (expected 4)", order));
        }
        if alphabet_size != EXPECTED_ALPHABET_SIZE {
            return Err(format!(
                "unsupported alphabet size {} (expected 27)",
                alphabet_size
            ));
        }

        // --- Alphabet (u16 BE per code point — upstream serializer is big-endian) ---
        let alphabet_bytes = alphabet_size * 2;
        if bytes.len() < cursor + alphabet_bytes {
            return Err("file too short for alphabet".into());
        }
        for i in 0..alphabet_size {
            let hi = bytes[cursor + i * 2] as u16;
            let lo = bytes[cursor + i * 2 + 1] as u16;
            let code_point = (hi << 8) | lo;
            let expected: u16 = if i == 0 {
                0x0020
            } else {
                (b'a' + (i as u8) - 1) as u16
            };
            if code_point != expected {
                return Err(format!(
                    "unexpected alphabet code point at slot {}: 0x{:04x} (expected 0x{:04x})",
                    i, code_point, expected
                ));
            }
        }
        cursor += alphabet_bytes;

        // --- Segments ---
        // For order N, there are size^(N-1) segments, each of length size.
        let segment_count = alphabet_size.pow((order - 1) as u32);
        let dense_len = segment_count * alphabet_size;
        let mut data = vec![0u8; dense_len];

        for seg in 0..segment_count {
            if cursor >= bytes.len() {
                return Err(format!("file truncated at segment {}", seg));
            }
            let entry_count = bytes[cursor] as usize;
            cursor += 1;
            if entry_count > alphabet_size {
                return Err(format!(
                    "segment {} has {} entries (max {})",
                    seg, entry_count, alphabet_size
                ));
            }
            if bytes.len() < cursor + entry_count * 2 {
                return Err(format!("file truncated in segment {} body", seg));
            }
            let segment_start = seg * alphabet_size;
            for _ in 0..entry_count {
                let index = bytes[cursor] as usize;
                let freq = bytes[cursor + 1];
                cursor += 2;
                if index >= alphabet_size {
                    return Err(format!(
                        "segment {} entry has alphabet index {} (max {})",
                        seg,
                        index,
                        alphabet_size - 1
                    ));
                }
                if freq == 0 {
                    return Err(format!(
                        "segment {} contains zero-frequency entry (sparse format must omit)",
                        seg
                    ));
                }
                data[segment_start + index] = freq;
            }
        }

        if cursor != bytes.len() {
            return Err(format!(
                "trailing {} unused bytes after segments",
                bytes.len() - cursor
            ));
        }

        Ok(Self {
            order,
            alphabet_size,
            data,
        })
    }

    /// Order of the Markov chain (currently always 4).
    #[inline]
    #[allow(dead_code)]
    pub fn order(&self) -> usize {
        self.order
    }

    /// Number of symbols in the alphabet (currently always 27).
    #[inline]
    #[allow(dead_code)]
    pub fn alphabet_size(&self) -> usize {
        self.alphabet_size
    }

    /// Convert a character to its alphabet index.
    ///
    /// Returns `Some(0)` for space, `Some(1..=26)` for `'a'..='z'`,
    /// and `None` for any character outside the alphabet.
    #[inline]
    pub fn char_to_idx(c: char) -> Option<usize> {
        if c == ' ' {
            Some(0)
        } else if c.is_ascii_lowercase() {
            Some((c as u8 - b'a' + 1) as usize)
        } else {
            None
        }
    }

    /// Convert an alphabet index back to a character.
    ///
    /// `0` → space, `1..=26` → `'a'..='z'`. Panics if `i > 26`.
    #[inline]
    pub fn idx_to_char(i: usize) -> char {
        if i == 0 {
            ' '
        } else if i <= 26 {
            (b'a' + (i as u8) - 1) as char
        } else {
            panic!("alphabet index {} out of range", i)
        }
    }

    /// Look up the frequency distribution for the next character given
    /// the last `order - 1` characters of history (in chronological order
    /// — oldest first).
    ///
    /// Returns a 27-byte slice indexed by next-character alphabet index.
    /// Frequencies in the slice sum to 255 for non-empty segments, or
    /// 0 for histories that never occurred in the training corpus.
    #[inline]
    pub fn segment(&self, history: &[usize]) -> &[u8] {
        debug_assert_eq!(history.len(), self.order - 1);
        let mut offset = 0usize;
        for &c in history {
            debug_assert!(c < self.alphabet_size);
            offset = offset * self.alphabet_size + c;
        }
        let segment_start = offset * self.alphabet_size;
        &self.data[segment_start..segment_start + self.alphabet_size]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_model() {
        let table = TransitionTable::from_embedded();
        assert_eq!(table.order(), 4);
        assert_eq!(table.alphabet_size(), 27);
    }

    #[test]
    fn char_idx_roundtrip() {
        assert_eq!(TransitionTable::char_to_idx(' '), Some(0));
        assert_eq!(TransitionTable::idx_to_char(0), ' ');
        for c in 'a'..='z' {
            let idx = TransitionTable::char_to_idx(c).unwrap();
            assert_eq!(TransitionTable::idx_to_char(idx), c);
        }
        assert_eq!(TransitionTable::char_to_idx('A'), None);
        assert_eq!(TransitionTable::char_to_idx('!'), None);
    }

    #[test]
    fn every_segment_sums_to_255_or_0() {
        // Each 27-byte segment is either a normalized frequency
        // distribution (sums to 255) or all-zero for an impossible
        // history that never appeared during training.
        let table = TransitionTable::from_embedded();
        let n = table.alphabet_size;
        let segments = n.pow((table.order - 1) as u32);
        for seg in 0..segments {
            let slice = &table.data[seg * n..(seg + 1) * n];
            let sum: u32 = slice.iter().map(|&b| b as u32).sum();
            assert!(
                sum == 0 || sum == 255,
                "segment {} sums to {} (expected 0 or 255)",
                seg,
                sum
            );
        }
    }

    #[test]
    fn segment_lookup_matches_indexing_formula() {
        let table = TransitionTable::from_embedded();
        // For order=4, history is 3 chars: oldest, middle, newest.
        // Take a likely-frequent prefix " th" (space, t, h) and
        // confirm the segment is non-empty and that 'e' is the most
        // likely follow-up (forming "the" — the most common English word).
        let history = [
            TransitionTable::char_to_idx(' ').unwrap(),
            TransitionTable::char_to_idx('t').unwrap(),
            TransitionTable::char_to_idx('h').unwrap(),
        ];
        let seg = table.segment(&history);
        let sum: u32 = seg.iter().map(|&b| b as u32).sum();
        assert_eq!(sum, 255, "_th segment should be a populated distribution");

        let e_idx = TransitionTable::char_to_idx('e').unwrap();
        let max_idx = seg
            .iter()
            .enumerate()
            .max_by_key(|(_, &v)| v)
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(
            max_idx, e_idx,
            "most likely follow-up to '_th' should be 'e' (got idx {})",
            max_idx
        );
    }
}
