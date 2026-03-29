// 27×27 bigram frequency table: index 0 = space (word boundary), 1–26 = a–z.
// Each row sums to 255 (byte-scaled probability distribution).
// Generated from the top 10,000 most frequent English words.
const BIGRAM_TABLE: [[u8; 27]; 27] = [
    [
        0, 18, 14, 26, 14, 12, 11, 7, 8, 10, 3, 3, 10, 14, 6, 6, 20, 1, 15, 26, 14, 3, 5, 7, 0, 1,
        1,
    ], // _
    [
        14, 0, 6, 15, 11, 0, 2, 7, 1, 10, 0, 3, 31, 10, 35, 0, 8, 1, 32, 15, 37, 4, 4, 2, 1, 5, 1,
    ], // a
    [
        12, 36, 4, 2, 1, 38, 0, 0, 0, 29, 2, 0, 33, 2, 1, 29, 1, 0, 26, 11, 2, 22, 0, 1, 0, 3, 0,
    ], // b
    [
        14, 31, 0, 5, 1, 33, 0, 0, 27, 16, 0, 13, 11, 0, 0, 49, 0, 1, 11, 4, 26, 10, 0, 0, 0, 3, 0,
    ], // c
    [
        90, 15, 1, 1, 4, 50, 0, 3, 0, 37, 1, 0, 3, 2, 1, 11, 0, 0, 7, 12, 1, 9, 3, 1, 0, 3, 0,
    ], // d
    [
        44, 13, 2, 11, 23, 6, 3, 3, 1, 1, 0, 1, 12, 8, 28, 1, 4, 1, 37, 32, 9, 1, 4, 2, 6, 2, 0,
    ], // e
    [
        17, 28, 0, 1, 1, 33, 21, 1, 0, 53, 0, 0, 17, 0, 0, 35, 0, 0, 17, 2, 8, 17, 0, 1, 0, 3, 0,
    ], // f
    [
        91, 18, 1, 0, 1, 41, 0, 3, 16, 15, 0, 0, 5, 1, 8, 9, 1, 0, 20, 9, 2, 10, 0, 0, 0, 4, 0,
    ], // g
    [
        33, 44, 1, 0, 1, 53, 0, 0, 0, 34, 0, 0, 3, 2, 3, 41, 1, 0, 8, 2, 12, 10, 0, 1, 0, 5, 1,
    ], // h
    [
        4, 12, 4, 21, 8, 13, 4, 8, 0, 1, 0, 1, 13, 8, 62, 28, 5, 0, 8, 21, 21, 1, 9, 0, 1, 0, 2,
    ], // i
    [
        14, 46, 0, 3, 1, 57, 0, 0, 0, 13, 1, 0, 0, 1, 0, 60, 4, 0, 1, 1, 0, 51, 2, 0, 0, 0, 0,
    ], // j
    [
        65, 12, 3, 0, 1, 67, 2, 1, 1, 41, 1, 0, 5, 1, 9, 5, 1, 0, 2, 29, 1, 3, 0, 0, 0, 5, 0,
    ], // k
    [
        39, 29, 1, 1, 7, 45, 2, 1, 0, 37, 0, 1, 24, 1, 1, 23, 1, 0, 0, 8, 7, 8, 2, 0, 0, 17, 0,
    ], // l
    [
        27, 45, 9, 1, 0, 58, 1, 0, 0, 34, 0, 0, 1, 9, 1, 26, 25, 0, 0, 8, 1, 6, 0, 0, 0, 3, 0,
    ], // m
    [
        44, 15, 1, 14, 19, 22, 3, 37, 1, 14, 1, 3, 1, 1, 5, 7, 0, 0, 0, 24, 35, 3, 3, 0, 0, 2, 0,
    ], // n
    [
        9, 4, 4, 7, 7, 1, 3, 5, 1, 2, 0, 3, 17, 16, 64, 9, 9, 0, 37, 11, 11, 17, 6, 8, 1, 2, 1,
    ], // o
    [
        18, 32, 0, 1, 1, 39, 0, 1, 12, 15, 0, 0, 23, 1, 1, 30, 12, 0, 39, 8, 11, 9, 0, 0, 0, 2, 0,
    ], // p
    [
        23, 2, 0, 2, 0, 0, 0, 0, 0, 2, 0, 0, 6, 0, 0, 0, 0, 0, 0, 2, 4, 214, 0, 0, 0, 0, 0,
    ], // q
    [
        32, 31, 2, 4, 7, 52, 1, 4, 0, 30, 0, 3, 3, 6, 5, 21, 2, 0, 6, 17, 12, 5, 3, 1, 0, 8, 0,
    ], // r
    [
        102, 7, 1, 7, 1, 24, 1, 0, 10, 18, 0, 2, 3, 2, 1, 9, 8, 0, 0, 13, 32, 11, 0, 1, 0, 2, 0,
    ], // s
    [
        41, 20, 1, 2, 0, 43, 0, 0, 14, 53, 0, 0, 4, 1, 1, 16, 1, 0, 19, 15, 7, 9, 0, 1, 0, 7, 0,
    ], // t
    [
        5, 13, 10, 12, 8, 11, 2, 9, 0, 11, 0, 1, 19, 15, 31, 1, 9, 0, 42, 29, 24, 0, 0, 0, 1, 1, 1,
    ], // u
    [
        7, 35, 1, 1, 1, 125, 0, 1, 0, 62, 0, 0, 0, 0, 0, 18, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0,
    ], // v
    [
        30, 52, 2, 1, 1, 42, 1, 0, 13, 43, 1, 0, 4, 1, 12, 25, 1, 0, 7, 13, 3, 0, 0, 1, 0, 2, 0,
    ], // w
    [
        57, 17, 1, 23, 0, 23, 1, 0, 6, 24, 0, 0, 1, 2, 1, 2, 51, 0, 0, 0, 30, 8, 0, 0, 4, 4, 0,
    ], // x
    [
        180, 4, 2, 3, 2, 12, 0, 1, 0, 6, 0, 0, 4, 6, 4, 5, 4, 0, 3, 12, 3, 1, 0, 2, 0, 0, 1,
    ], // y
    [
        32, 42, 2, 0, 4, 83, 0, 0, 0, 28, 0, 0, 4, 0, 0, 26, 0, 0, 2, 2, 0, 9, 0, 0, 0, 8, 13,
    ], // z
];

/// Accessor for the bigram transition table.
///
/// Provides methods to look up transition probabilities given a previous
/// character. Space is represented as index 0 (word boundary).
pub struct TransitionTable;

impl TransitionTable {
    /// Convert a character to its table index.
    /// Space (word boundary) = 0, 'a' = 1, ..., 'z' = 26.
    #[inline]
    pub fn char_to_index(c: char) -> usize {
        if c == ' ' {
            0
        } else {
            (c as u8 - b'a' + 1) as usize
        }
    }

    /// Convert a table index back to a character.
    /// 0 = space, 1 = 'a', ..., 26 = 'z'.
    #[inline]
    pub fn index_to_char(i: usize) -> char {
        if i == 0 {
            ' '
        } else {
            (b'a' + (i as u8) - 1) as char
        }
    }

    /// Get the full transition row for a given previous character.
    /// Returns all 27 frequency values (space + a–z).
    #[inline]
    pub fn get_row(prev: char) -> &'static [u8; 27] {
        &BIGRAM_TABLE[Self::char_to_index(prev)]
    }

    /// Get the transition frequency from `prev` to `next`.
    #[inline]
    pub fn get_freq(prev: char, next: char) -> u8 {
        BIGRAM_TABLE[Self::char_to_index(prev)][Self::char_to_index(next)]
    }

    /// Return a list of (char, frequency) pairs for all non-zero transitions
    /// from the given previous character.
    #[allow(dead_code)]
    pub fn get_candidates(prev: char) -> Vec<(char, u8)> {
        let row = Self::get_row(prev);
        row.iter()
            .enumerate()
            .filter(|(_, &freq)| freq > 0)
            .map(|(i, &freq)| (Self::index_to_char(i), freq))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_row_sums_to_255() {
        for (i, row) in BIGRAM_TABLE.iter().enumerate() {
            let sum: u16 = row.iter().map(|&v| v as u16).sum();
            assert_eq!(sum, 255, "row {} sums to {} instead of 255", i, sum);
        }
    }

    #[test]
    fn char_to_index_roundtrip() {
        assert_eq!(TransitionTable::char_to_index(' '), 0);
        assert_eq!(TransitionTable::index_to_char(0), ' ');
        for c in 'a'..='z' {
            let idx = TransitionTable::char_to_index(c);
            assert_eq!(TransitionTable::index_to_char(idx), c);
        }
    }

    #[test]
    fn space_does_not_follow_space() {
        // The space->space transition should be 0
        assert_eq!(TransitionTable::get_freq(' ', ' '), 0);
    }

    #[test]
    fn get_candidates_returns_nonzero_only() {
        let candidates = TransitionTable::get_candidates('a');
        for (_, freq) in &candidates {
            assert!(*freq > 0);
        }
    }
}
