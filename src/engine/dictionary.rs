use crate::engine::filter::LetterFilter;
use crate::engine::generator::SimpleRng;

/// Embedded English wordlist.
///
/// Source: keybr.com's `packages/keybr-content-words/lib/data/words-en.json`
/// (10,000 most common English words), normalized to ASCII lowercase a–z only,
/// length 2..=12, one word per line. See `data/README.md`.
const RAW_WORDLIST: &str = include_str!("../../data/wordlist-en.txt");

/// Dictionary of real English words, indexed for fast filter-aware lookup.
///
/// We bucket each word into the 26 per-letter buckets corresponding to the
/// distinct letters it contains. With a focused letter, the candidate pool
/// is the bucket for that letter; without one, all words are candidates.
/// At ~10K words a per-character allowed-set check is plenty fast — no need
/// for a bitmap index.
pub struct Dictionary {
    all_words: Vec<&'static str>,
    by_letter: [Vec<u32>; 26],
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::from_embedded()
    }
}

impl Dictionary {
    /// Build a `Dictionary` from the embedded wordlist.
    pub fn from_embedded() -> Self {
        // Default-construct the 26 per-letter buckets.
        let mut by_letter: [Vec<u32>; 26] = Default::default();
        let mut all_words: Vec<&'static str> = Vec::with_capacity(10_000);

        for raw in RAW_WORDLIST.lines() {
            let word = raw.trim();
            if word.is_empty() {
                continue;
            }
            // Defensive: reject any word that slipped through with
            // non-ASCII or non-lowercase characters.
            if !word.bytes().all(|b| b.is_ascii_lowercase()) {
                continue;
            }

            let idx = all_words.len() as u32;
            all_words.push(word);

            // Index by each distinct letter present in the word.
            let mut seen = [false; 26];
            for b in word.bytes() {
                let li = (b - b'a') as usize;
                if !seen[li] {
                    seen[li] = true;
                    by_letter[li].push(idx);
                }
            }
        }

        Self {
            all_words,
            by_letter,
        }
    }

    /// Total number of words in the dictionary.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.all_words.len()
    }

    /// True if the dictionary is empty. Paired with `len()` to satisfy
    /// the clippy lint that asks for both together.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.all_words.is_empty()
    }

    /// Pick a random word from the dictionary that:
    /// - contains the filter's focused key (if any), and
    /// - uses only characters in `filter.allowed` (plus space — but words
    ///   never contain space, so this is moot for individual words).
    ///
    /// Returns `None` if no word satisfies the filter. Callers should
    /// fall back to phonetic generation in that case.
    pub fn next_word(&self, filter: &LetterFilter, rng: &mut SimpleRng) -> Option<&'static str> {
        // 1. Choose the candidate pool: bucket for the focused letter,
        //    or all words if no focus is set.
        let pool: &[u32] = match filter.focused {
            Some(c) if c.is_ascii_lowercase() => &self.by_letter[(c as u8 - b'a') as usize],
            // Non-alphabetic focus (e.g. space) — fall back to all words.
            _ => {
                if self.all_words.is_empty() {
                    return None;
                }
                return self.sample_filtered(filter, rng, None);
            }
        };

        if pool.is_empty() {
            return None;
        }

        self.sample_filtered(filter, rng, Some(pool))
    }

    /// Sample a random word from either a focused pool of indices or all
    /// words, rejecting those whose characters aren't all in `filter.allowed`.
    ///
    /// Uses bounded rejection sampling: tries up to N random picks before
    /// scanning the full pool for any valid word. With ~10K words and a
    /// reasonable allowed-set this almost always succeeds on the first pick.
    fn sample_filtered(
        &self,
        filter: &LetterFilter,
        rng: &mut SimpleRng,
        pool: Option<&[u32]>,
    ) -> Option<&'static str> {
        let pool_len = match pool {
            Some(p) => p.len() as u32,
            None => self.all_words.len() as u32,
        };
        if pool_len == 0 {
            return None;
        }

        // First try a few random picks — fast path for permissive filters.
        const RANDOM_TRIES: u32 = 16;
        for _ in 0..RANDOM_TRIES {
            let pick = rng.next_bounded(pool_len);
            let word_idx = match pool {
                Some(p) => p[pick as usize] as usize,
                None => pick as usize,
            };
            let word = self.all_words[word_idx];
            if word_chars_allowed(word, filter) {
                return Some(word);
            }
        }

        // Fall back to scanning: collect all valid candidates, then pick one.
        // For ~10K words this is a few microseconds.
        let mut valid: Vec<&'static str> = Vec::new();
        match pool {
            Some(p) => {
                for &i in p {
                    let w = self.all_words[i as usize];
                    if word_chars_allowed(w, filter) {
                        valid.push(w);
                    }
                }
            }
            None => {
                for &w in &self.all_words {
                    if word_chars_allowed(w, filter) {
                        valid.push(w);
                    }
                }
            }
        }

        if valid.is_empty() {
            None
        } else {
            let pick = rng.next_bounded(valid.len() as u32) as usize;
            Some(valid[pick])
        }
    }
}

/// True if every character in `word` is allowed by `filter`. Words are
/// ASCII lowercase a–z by construction, so we don't need to consider
/// space or uppercase.
#[inline]
fn word_chars_allowed(word: &str, filter: &LetterFilter) -> bool {
    word.chars().all(|c| filter.is_allowed(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_wordlist() {
        let dict = Dictionary::from_embedded();
        assert!(
            dict.len() >= 1000,
            "expected dictionary to contain >= 1000 words, got {}",
            dict.len()
        );
    }

    #[test]
    fn all_words_are_ascii_lowercase() {
        let dict = Dictionary::from_embedded();
        for w in &dict.all_words {
            assert!(w.bytes().all(|b| b.is_ascii_lowercase()), "bad word: {w}");
            assert!(!w.is_empty());
        }
    }

    #[test]
    fn next_word_respects_filter() {
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = vec!['e', 't', 'a', 'o', 'i', 'n', 'r', 's', 'h', 'l'];
        let filter = LetterFilter::new(&allowed, Some('e'));
        let mut rng = SimpleRng::with_seed(42);

        // Try many times — every result must satisfy the filter.
        for _ in 0..100 {
            let word = dict
                .next_word(&filter, &mut rng)
                .expect("filter should have at least one matching word");
            assert!(
                word.contains('e'),
                "focused key 'e' missing from word '{word}'",
            );
            for c in word.chars() {
                assert!(
                    allowed.contains(&c),
                    "word '{word}' contains disallowed char '{c}'",
                );
            }
        }
    }

    #[test]
    fn returns_none_when_no_match() {
        let dict = Dictionary::from_embedded();
        // Impossible filter: only 'q' and 'z' allowed, with no focus.
        // No English word in the list uses only these letters.
        let filter = LetterFilter::new(&['q', 'z'], None);
        let mut rng = SimpleRng::with_seed(42);
        assert!(dict.next_word(&filter, &mut rng).is_none());
    }

    #[test]
    fn returns_none_with_focus_outside_alphabet() {
        let dict = Dictionary::from_embedded();
        // Filter focused on 'q' but allowing only 'a' and 'b' — no word
        // contains 'q' here AND uses only a/b.
        let filter = LetterFilter::new(&['a', 'b'], Some('q'));
        let mut rng = SimpleRng::with_seed(42);
        assert!(dict.next_word(&filter, &mut rng).is_none());
    }

    #[test]
    fn unfocused_filter_returns_word() {
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = ('a'..='z').collect();
        let filter = LetterFilter::new(&allowed, None);
        let mut rng = SimpleRng::with_seed(7);
        let word = dict.next_word(&filter, &mut rng);
        assert!(word.is_some());
    }
}
