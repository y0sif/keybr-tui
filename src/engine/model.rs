//! The keybr.com phonetic model file: its format and a checked reader.
//!
//! [`TransitionTable`](super::transition::TransitionTable) decodes a whole
//! file into its dense table, and [`ModelLetters`] reads only the letters,
//! so a language knows them without building one. Both walk the file with
//! `read_header` and `read_segments`, so they accept and reject the same
//! files.
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

/// ASCII signature at the start of every model file.
pub(super) const SIGNATURE: &[u8] = b"keybr.com";

/// Markov chain order we expect from the upstream models.
const EXPECTED_ORDER: usize = 4;

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
pub(super) struct Header {
    pub(super) order: usize,
    /// Space, then the letters.
    pub(super) alphabet: Vec<char>,
    /// Offset of the first segment.
    body: usize,
}

/// Read the signature, chain header and alphabet. The alphabet is the
/// file's own, so it is checked for its shape: space first, then distinct
/// letters that are not uppercase (caseless scripts pass).
pub(super) fn read_header(bytes: &[u8]) -> Result<Header, String> {
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
pub(super) fn read_segments(
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
