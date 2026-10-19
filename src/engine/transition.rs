//! Order-4 Markov phonetic transition model.
//!
//! Ported from upstream keybr.com
//! (`packages/keybr-phonetic-model/lib/transitiontable.ts`).
//!
//! Each language embeds one binary model from keybr.com
//! (`data/model-<code>.data`, see [`Language::model_bytes`]); `model-en.data`
//! is the phonetic transition table trained on English text. Compared to the previous
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
//! [1 byte]  alphabet size (27 for English: space + a..z)
//! [size*2]  alphabet code points (u16 BE): 0x0020 (space), then the letters
//! [...]     sparse segment data, one segment per (order-1)-char history:
//!             [1 byte] segment length N (entries with non-zero frequency)
//!             [N*2]    pairs of (alphabet_index u8, frequency u8)
//! ```
//!
//! Frequencies in each segment sum to 255 (or 0 for impossible histories).
//!
//! The alphabet header is the language's own alphabet (keybr.com writes it
//! as `[space, ...Language.alphabet]`), so the letters and their unlock
//! order are read from the file ([`ModelLetters`]) rather than kept beside
//! it, and a header is checked for its shape instead of against a list.
//!
//! At load time we decode the sparse representation into a dense
//! `Vec<u8>` of length `alphabet_size^order` (≈ 519 KiB for English),
//! flattened in row-major order so segment lookup is a single index
//! computation: `offset = ((h0 * size) + h1) * size + h2`.

use std::collections::HashMap;

use super::language::Language;

/// ASCII signature at the start of every model file.
const SIGNATURE: &[u8] = b"keybr.com";

/// Markov chain order we expect from the upstream models.
const EXPECTED_ORDER: usize = 4;

/// Decoded order-4 transition table.
///
/// Once loaded, lookup is O(1): a segment is a contiguous
/// `alphabet_size`-byte slice containing the next-character frequency
/// distribution given the last `order - 1` characters.
pub struct TransitionTable {
    /// Markov chain order (= 4 for every upstream model).
    order: usize,
    /// Number of distinct symbols: space plus the language's letters.
    alphabet_size: usize,
    /// Symbol for each alphabet index; index 0 is always space.
    alphabet: Vec<char>,
    /// Reverse of `alphabet`.
    index: HashMap<char, usize>,
    /// Densely decoded segment data: `alphabet_size.pow(order)` bytes.
    ///
    /// Indexing: for history `[h0, h1, ..., h_{order-2}]` the segment
    /// starts at offset `segment_index(history) * alphabet_size`.
    data: Vec<u8>,
}

impl TransitionTable {
    /// Decode the embedded English model.
    #[cfg(test)]
    pub fn from_embedded() -> Self {
        Self::for_language(Language::English)
    }

    /// Decode the embedded model for `language`.
    ///
    /// Panics with a descriptive message if the binary asset is missing
    /// or malformed — this is a build-time/asset failure and not
    /// something the runtime should attempt to recover from.
    pub fn for_language(language: Language) -> Self {
        Self::from_bytes(language.model_bytes())
            .unwrap_or_else(|e| panic!("embedded model-{}.data is malformed: {e}", language.code()))
    }

    /// Decode a transition table from a raw byte slice. The alphabet is the
    /// file's own; see `read_header` for what makes it well-formed.
    ///
    /// Returns an error string describing the first malformed field
    /// encountered, suitable for `expect()` in callers.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let header = read_header(bytes)?;
        let alphabet_size = header.alphabet.len();

        // For order N, there are size^(N-1) segments, each of length size.
        let segment_count = alphabet_size.pow((header.order - 1) as u32);
        let mut data = vec![0u8; segment_count * alphabet_size];
        read_segments(bytes, &header, |seg, index, freq| {
            data[seg * alphabet_size + index] = freq;
        })?;

        let index = header
            .alphabet
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i))
            .collect();
        Ok(Self {
            order: header.order,
            alphabet_size,
            alphabet: header.alphabet,
            index,
            data,
        })
    }

    /// Order of the Markov chain (currently always 4).
    #[inline]
    #[allow(dead_code)]
    pub fn order(&self) -> usize {
        self.order
    }

    /// Number of symbols in the alphabet (space plus the letters).
    #[inline]
    #[allow(dead_code)]
    pub fn alphabet_size(&self) -> usize {
        self.alphabet_size
    }

    /// Alphabet index of space, the word boundary.
    pub const SPACE: usize = 0;

    /// Convert a character to its alphabet index.
    ///
    /// Returns `Some(0)` for space, `Some(1..)` for the letters in model
    /// order, and `None` for any character outside the alphabet.
    #[inline]
    pub fn char_to_idx(&self, c: char) -> Option<usize> {
        self.index.get(&c).copied()
    }

    /// Convert an alphabet index back to a character.
    ///
    /// `0` → space. Panics if `i` is out of range.
    #[inline]
    pub fn idx_to_char(&self, i: usize) -> char {
        self.alphabet[i]
    }

    /// Summed frequency of every letter across all segments: keybr.com's
    /// `TransitionTable.letters`, which drives the unlock order.
    #[cfg(test)]
    pub fn letter_totals(&self) -> Vec<(char, u64)> {
        let mut totals = vec![0u64; self.alphabet_size];
        for segment in self.data.chunks(self.alphabet_size) {
            for (i, &f) in segment.iter().enumerate() {
                totals[i] += u64::from(f);
            }
        }
        totals
            .into_iter()
            .enumerate()
            .skip(1)
            .map(|(i, f)| (self.alphabet[i], f))
            .collect()
    }

    /// Look up the frequency distribution for the next character given
    /// the last `order - 1` characters of history (in chronological order
    /// — oldest first).
    ///
    /// Returns an `alphabet_size`-byte slice indexed by next-character
    /// alphabet index.
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

/// The letters a model teaches, read in one pass over its bytes without
/// building the dense table, so a language knows them before (or without)
/// any generator.
pub struct ModelLetters {
    /// The alphabet after its leading space, in header order: keybr.com's
    /// `Language.alphabet`, the display order.
    pub letters: Vec<char>,
    /// keybr.com's `Letter.frequencyOrder` over `TransitionTable.letters`:
    /// each letter's frequencies summed over every segment, highest total
    /// first, ties by code point.
    pub frequency_order: Vec<char>,
}

impl ModelLetters {
    /// Read the letters of a model file, checking the whole file the same
    /// way `TransitionTable::from_bytes` does.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let header = read_header(bytes)?;
        let alphabet = &header.alphabet;
        let mut totals = vec![0u64; alphabet.len()];
        read_segments(bytes, &header, |_, index, freq| {
            totals[index] += u64::from(freq);
        })?;
        // Index 0 is space, which keybr.com drops before sorting.
        let mut order: Vec<usize> = (1..alphabet.len()).collect();
        order.sort_by(|&a, &b| {
            totals[b]
                .cmp(&totals[a])
                .then(alphabet[a].cmp(&alphabet[b]))
        });
        Ok(Self {
            letters: alphabet[1..].to_vec(),
            frequency_order: order.into_iter().map(|i| alphabet[i]).collect(),
        })
    }
}

/// Signature, chain header and alphabet of a model file.
struct Header {
    order: usize,
    /// Space, then the letters.
    alphabet: Vec<char>,
    /// Offset of the first segment.
    body: usize,
}

/// Read the signature, chain header and alphabet. The alphabet is the
/// file's own, so it is checked for its shape: space first, then distinct
/// letters that are not uppercase (caseless scripts pass).
fn read_header(bytes: &[u8]) -> Result<Header, String> {
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
    if alphabet_size < 2 {
        return Err(format!(
            "unsupported alphabet size {} (expected space and at least one letter)",
            alphabet_size
        ));
    }

    // --- Alphabet (u16 BE per code point — upstream serializer is big-endian) ---
    let alphabet_bytes = alphabet_size * 2;
    if bytes.len() < cursor + alphabet_bytes {
        return Err(format!(
            "file too short for an alphabet of {} code points",
            alphabet_size
        ));
    }
    let mut alphabet: Vec<char> = Vec::with_capacity(alphabet_size);
    for i in 0..alphabet_size {
        let at = cursor + i * 2;
        let code_point = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        let c = char::from_u32(u32::from(code_point))
            .ok_or_else(|| format!("alphabet slot {i} is 0x{code_point:04x}, not a character"))?;
        if i == 0 && c != ' ' {
            return Err(format!(
                "alphabet slot 0 is U+{code_point:04X} {c:?}, expected space"
            ));
        }
        if i > 0 && (!c.is_alphabetic() || c.is_uppercase()) {
            return Err(format!(
                "alphabet slot {i} is U+{code_point:04X} {c:?}, not a lowercase letter"
            ));
        }
        if let Some(first) = alphabet.iter().position(|&a| a == c) {
            return Err(format!(
                "alphabet slot {i} repeats U+{code_point:04X} {c:?} from slot {first}"
            ));
        }
        alphabet.push(c);
    }
    cursor += alphabet_bytes;

    Ok(Header {
        order,
        alphabet,
        body: cursor,
    })
}

/// Walk the sparse segments after `header` to the end of the file, checking
/// each one, and hand every entry to `visit` as
/// `(segment, alphabet index, frequency)`.
fn read_segments(
    bytes: &[u8],
    header: &Header,
    mut visit: impl FnMut(usize, usize, u8),
) -> Result<(), String> {
    let alphabet_size = header.alphabet.len();
    let mut cursor = header.body;

    // For order N, there are size^(N-1) segments, each of length size.
    let segment_count = alphabet_size.pow((header.order - 1) as u32);
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
            visit(seg, index, freq);
        }
    }

    if cursor != bytes.len() {
        return Err(format!(
            "trailing {} unused bytes after segments",
            bytes.len() - cursor
        ));
    }
    Ok(())
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
        let table = TransitionTable::from_embedded();
        assert_eq!(table.char_to_idx(' '), Some(0));
        assert_eq!(table.idx_to_char(0), ' ');
        for c in 'a'..='z' {
            let idx = table.char_to_idx(c).unwrap();
            assert_eq!(table.idx_to_char(idx), c);
        }
        assert_eq!(table.char_to_idx('A'), None);
        assert_eq!(table.char_to_idx('!'), None);
        assert_eq!(table.char_to_idx('ş'), None);
    }

    #[test]
    fn loads_turkish_model() {
        let table = TransitionTable::for_language(Language::Turkish);
        assert_eq!(table.order(), 4);
        assert_eq!(table.alphabet_size(), 30);
        for &c in Language::Turkish.letters() {
            let idx = table
                .char_to_idx(c)
                .expect("every Turkish letter is in the model");
            assert_eq!(table.idx_to_char(idx), c);
        }
        assert_eq!(table.char_to_idx('q'), None);
        assert_eq!(table.char_to_idx('w'), None);
    }

    #[test]
    fn loads_german_model() {
        let table = TransitionTable::for_language(Language::German);
        assert_eq!(table.alphabet_size(), 31);
        for c in ['ä', 'ö', 'ü', 'ß'] {
            let idx = table.char_to_idx(c).expect("umlauts and ß are letters");
            assert_eq!(table.idx_to_char(idx), c);
        }
        assert_eq!(table.char_to_idx('ẞ'), None);
    }

    /// The English model with alphabet slot `slot` set to `c`.
    fn with_alphabet_slot(slot: usize, c: char) -> Vec<u8> {
        let mut bytes = Language::English.model_bytes().to_vec();
        let at = SIGNATURE.len() + 2 + slot * 2;
        bytes[at..at + 2].copy_from_slice(&(c as u16).to_be_bytes());
        bytes
    }

    #[test]
    fn rejects_a_malformed_alphabet() {
        let cases = [
            (with_alphabet_slot(0, 'a'), "expected space"),
            (with_alphabet_slot(2, 'a'), "repeats U+0061 'a' from slot 1"),
            (with_alphabet_slot(5, 'E'), "not a lowercase letter"),
            (with_alphabet_slot(5, '-'), "not a lowercase letter"),
            (with_alphabet_slot(5, ' '), "not a lowercase letter"),
        ];
        for (bytes, expected) in cases {
            let err = TransitionTable::from_bytes(&bytes)
                .err()
                .expect("a malformed alphabet must not load");
            assert!(err.contains(expected), "{err}");
            let err = ModelLetters::read(&bytes).err().expect("nor yield letters");
            assert!(err.contains(expected), "{err}");
        }

        // A size byte that disagrees with the data runs the segments off
        // their boundaries.
        let mut bytes = Language::English.model_bytes().to_vec();
        bytes[SIGNATURE.len() + 1] = 26;
        assert!(TransitionTable::from_bytes(&bytes).is_err());
        bytes[SIGNATURE.len() + 1] = 1;
        let err = ModelLetters::read(&bytes).err().unwrap();
        assert!(err.contains("alphabet size 1"), "{err}");
    }

    #[test]
    fn turkish_segment_lookup_is_plausible() {
        // "bir" is the most common Turkish word; after " bi" the model
        // should rate 'r' as the most likely next letter.
        let table = TransitionTable::for_language(Language::Turkish);
        let history = [
            table.char_to_idx(' ').unwrap(),
            table.char_to_idx('b').unwrap(),
            table.char_to_idx('i').unwrap(),
        ];
        let seg = table.segment(&history);
        let max_idx = seg
            .iter()
            .enumerate()
            .max_by_key(|(_, &v)| v)
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(table.idx_to_char(max_idx), 'r');
    }

    #[test]
    fn every_segment_sums_to_255_or_0() {
        // Each segment is either a normalized frequency
        // distribution (sums to 255) or all-zero for an impossible
        // history that never appeared during training.
        for &lang in Language::ALL {
            let table = TransitionTable::for_language(lang);
            let n = table.alphabet_size;
            let segments = n.pow((table.order - 1) as u32);
            for seg in 0..segments {
                let slice = &table.data[seg * n..(seg + 1) * n];
                let sum: u32 = slice.iter().map(|&b| b as u32).sum();
                assert!(
                    sum == 0 || sum == 255,
                    "{} segment {} sums to {} (expected 0 or 255)",
                    lang.code(),
                    seg,
                    sum
                );
            }
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
            table.char_to_idx(' ').unwrap(),
            table.char_to_idx('t').unwrap(),
            table.char_to_idx('h').unwrap(),
        ];
        let seg = table.segment(&history);
        let sum: u32 = seg.iter().map(|&b| b as u32).sum();
        assert_eq!(sum, 255, "_th segment should be a populated distribution");

        let e_idx = table.char_to_idx('e').unwrap();
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
