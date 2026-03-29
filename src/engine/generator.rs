use super::filter::LetterFilter;
use super::transition::TransitionTable;

/// Simple deterministic PRNG (xorshift32) — no external rand crate needed.
pub struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u32;
        Self { state: seed | 1 } // ensure non-zero
    }

    #[cfg(test)]
    pub fn with_seed(seed: u32) -> Self {
        Self { state: seed | 1 }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    /// Return a random u32 in [0, bound).
    pub fn next_bounded(&mut self, bound: u32) -> u32 {
        self.next_u32() % bound
    }
}

/// Word generator using a Markov chain driven by the transition table.
pub struct WordGenerator {
    rng: SimpleRng,
}

const MAX_WORD_LEN: usize = 10;
const MIN_WORD_LEN: usize = 3;
const MAX_RETRIES: usize = 5;
const SPACE_BOOST_BASE: f64 = 1.3;

impl WordGenerator {
    pub fn new() -> Self {
        Self {
            rng: SimpleRng::new(),
        }
    }

    #[cfg(test)]
    pub fn with_seed(seed: u32) -> Self {
        Self {
            rng: SimpleRng::with_seed(seed),
        }
    }

    /// Generate a single word respecting the letter filter.
    ///
    /// Algorithm (matching keybr.com):
    /// 1. If there's a focused key, find prefixes (up to length 3) containing it
    /// 2. Pick a random prefix as seed
    /// 3. At each step: get transition segment for last char, filter to allowed keys
    /// 4. If word shorter than 3 chars, remove space from candidates
    /// 5. Boost space frequency by 1.3^word_length (bias toward shorter words)
    /// 6. Weighted random select next char
    /// 7. Max word length 10, retry up to 5 times on dead-ends
    pub fn next_word(&mut self, filter: &LetterFilter) -> String {
        for _ in 0..MAX_RETRIES {
            if let Some(word) = self.try_generate_word(filter) {
                return word;
            }
        }
        // Fallback: return a single focused key or first allowed key repeated
        let c = filter
            .focused
            .unwrap_or_else(|| *filter.allowed.iter().next().unwrap_or(&'e'));
        std::iter::repeat(c).take(MIN_WORD_LEN).collect()
    }

    fn try_generate_word(&mut self, filter: &LetterFilter) -> Option<String> {
        let mut word = String::with_capacity(MAX_WORD_LEN);

        // Step 1-2: Seed the word
        if let Some(focused) = filter.focused {
            // Find valid prefixes containing the focused key (up to length 3)
            let prefix = self.find_prefix_with_key(filter, focused);
            word.push_str(&prefix);
        } else {
            // Start from space (word boundary) — pick first char
            let first = self.sample_next(' ', filter, 0)?;
            word.push(first);
        }

        // Step 3-7: Extend the word
        loop {
            if word.len() >= MAX_WORD_LEN {
                break;
            }

            let prev = word.chars().last().unwrap();
            match self.sample_next(prev, filter, word.len()) {
                Some(' ') => break, // space = end of word
                Some(c) => word.push(c),
                None => return None, // dead end, retry
            }
        }

        // Ensure focused key appears in the word
        if let Some(focused) = filter.focused {
            if !word.contains(focused) {
                return None; // retry
            }
        }

        if word.len() < MIN_WORD_LEN {
            return None;
        }

        Some(word)
    }

    /// Find a prefix (1-3 chars) that contains the focused key and respects
    /// the transition table probabilities.
    fn find_prefix_with_key(&mut self, filter: &LetterFilter, focused: char) -> String {
        // Try to find a 1-3 char prefix containing the focused key
        // Strategy: try starting with the focused key directly
        // or find a 2-char prefix where focused appears at position 0 or 1

        // Simplest: just start with the focused key if it can follow a space
        let space_to_focused = TransitionTable::get_freq(' ', focused);
        if space_to_focused > 0 {
            // Try prefix of length 1-3 starting with focused
            if self.rng.next_bounded(2) == 0 {
                return focused.to_string();
            }
        }

        // Try a 2-char prefix: find chars that can precede the focused key
        // and that can follow a space
        let mut candidates: Vec<(char, u32)> = Vec::new();
        for &c in &filter.allowed.iter().copied().collect::<Vec<_>>() {
            let space_to_c = TransitionTable::get_freq(' ', c) as u32;
            let c_to_focused = TransitionTable::get_freq(c, focused) as u32;
            if space_to_c > 0 && c_to_focused > 0 {
                candidates.push((c, space_to_c * c_to_focused));
            }
        }

        if !candidates.is_empty() && self.rng.next_bounded(3) > 0 {
            let first = self.weighted_sample(&candidates);
            let mut prefix = String::with_capacity(2);
            prefix.push(first);
            prefix.push(focused);
            return prefix;
        }

        // Fallback: just use the focused key
        focused.to_string()
    }

    /// Sample the next character given the previous one, respecting the filter.
    ///
    /// Returns None if no valid candidates exist (dead end).
    /// Returns Some(' ') to signal end of word.
    fn sample_next(&mut self, prev: char, filter: &LetterFilter, word_len: usize) -> Option<char> {
        let row = TransitionTable::get_row(prev);

        // Build filtered candidates: (char, adjusted_frequency)
        let mut candidates: Vec<(char, u32)> = Vec::new();

        for (i, &freq_val) in row.iter().enumerate() {
            let freq = freq_val as u32;
            if freq == 0 {
                continue;
            }
            let c = TransitionTable::index_to_char(i);

            if c == ' ' {
                // Step 4: If word shorter than MIN_WORD_LEN, skip space
                if word_len < MIN_WORD_LEN {
                    continue;
                }
                // Step 5: Boost space frequency by 1.3^word_length
                let boosted = (freq as f64 * SPACE_BOOST_BASE.powi(word_len as i32)) as u32;
                candidates.push((' ', boosted.max(1)));
            } else if filter.is_allowed(c) {
                candidates.push((c, freq));
            }
        }

        if candidates.is_empty() {
            return None;
        }

        Some(self.weighted_sample(&candidates))
    }

    /// Weighted random sample from a list of (char, weight) pairs.
    fn weighted_sample(&mut self, weights: &[(char, u32)]) -> char {
        let total: u32 = weights.iter().map(|(_, w)| w).sum();
        if total == 0 {
            return weights[0].0;
        }
        let mut pick = self.rng.next_bounded(total);
        for &(c, w) in weights {
            if pick < w {
                return c;
            }
            pick -= w;
        }
        weights.last().unwrap().0
    }

    /// Generate a fragment of text (multiple unique words separated by spaces)
    /// targeting approximately `target_len` characters.
    pub fn generate_fragment(&mut self, filter: &LetterFilter, target_len: usize) -> String {
        let mut result = String::with_capacity(target_len + 20);
        let mut seen = std::collections::HashSet::new();
        let mut stuck_counter = 0;

        while result.len() < target_len {
            let word = self.next_word(filter);

            // Try to avoid duplicates, but don't loop forever
            if seen.contains(&word) {
                stuck_counter += 1;
                if stuck_counter > 10 {
                    // Allow duplicates if we're stuck
                    stuck_counter = 0;
                } else {
                    continue;
                }
            }

            seen.insert(word.clone());

            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&word);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter(keys: &[char]) -> LetterFilter {
        LetterFilter::new(keys, None)
    }

    fn make_filter_focused(keys: &[char], focused: char) -> LetterFilter {
        LetterFilter::new(keys, Some(focused))
    }

    #[test]
    fn generated_text_only_contains_allowed_letters_and_spaces() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 100);

        for ch in text.chars() {
            assert!(
                ch == ' ' || keys.contains(&ch),
                "unexpected character '{}' in generated text: {}",
                ch,
                text
            );
        }
    }

    #[test]
    fn generated_text_is_long_enough() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 55);
        assert!(text.len() >= 55, "text too short: {} chars", text.len());
    }

    #[test]
    fn generated_text_contains_spaces() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 100);
        assert!(
            text.contains(' '),
            "generated text should contain spaces between words"
        );
    }

    #[test]
    fn focused_key_appears_in_every_word() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n', 's'];
        let filter = make_filter_focused(&keys, 's');
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 100);

        for word in text.split_whitespace() {
            assert!(
                word.contains('s'),
                "focused key 's' missing from word '{}' in: {}",
                word,
                text
            );
        }
    }

    #[test]
    fn words_are_at_least_3_chars() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 100);

        for word in text.split_whitespace() {
            assert!(
                word.len() >= 3,
                "word '{}' is shorter than 3 chars in: {}",
                word,
                text
            );
        }
    }

    #[test]
    fn words_are_at_most_10_chars() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n', 's', 'r', 'h', 'l'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 200);

        for word in text.split_whitespace() {
            assert!(
                word.len() <= 10,
                "word '{}' exceeds 10 chars in: {}",
                word,
                text
            );
        }
    }

    #[test]
    fn simple_rng_produces_different_values() {
        let mut rng = SimpleRng::with_seed(42);
        let a = rng.next_u32();
        let b = rng.next_u32();
        let c = rng.next_u32();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    #[test]
    fn generated_text_with_two_keys() {
        let keys = vec!['e', 't'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let text = gen.generate_fragment(&filter, 55);
        for ch in text.chars() {
            assert!(
                ch == ' ' || ch == 'e' || ch == 't',
                "unexpected character '{}' with two active keys",
                ch
            );
        }
    }
}
