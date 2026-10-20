use std::borrow::Cow;

use super::dictionary::Dictionary;
use super::filter::{FocusRule, LetterFilter};
use super::language::Language;
use super::transition::TransitionTable;

/// Simple deterministic PRNG (xorshift32) — no external rand crate needed.
pub struct SimpleRng {
    state: u32,
}

impl Default for SimpleRng {
    fn default() -> Self {
        Self::new()
    }
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

/// Word generator using an order-4 Markov chain driven by the embedded
/// phonetic transition table.
///
/// Each next character is sampled from the distribution conditioned on
/// the previous three characters (with spaces as word boundaries).
/// Compared to the prior order-2 bigram model, this produces output
/// that reads as plausible words of the model's language at filter
/// boundaries.
pub struct WordGenerator {
    rng: SimpleRng,
    table: TransitionTable,
    dictionary: Dictionary,
    /// When true, a lesson draws from a pool of real dictionary words
    /// that fit the filter, padded with phonetic words when too few do
    /// (see `natural_word_pool`). When false, every word is phonetic.
    /// Combination drills read the dictionary either way.
    natural_words: bool,
}

const MAX_WORD_LEN: usize = 10;
const MIN_WORD_LEN: usize = 3;
const MAX_RETRIES: usize = 5;
const SPACE_BOOST_BASE: f64 = 1.3;

/// History length is `order - 1`. For the order-4 models this is 3.
const HISTORY_LEN: usize = 3;

/// The natural-words pool, as keybr.com's `GuidedLesson` builds it: at
/// most this many dictionary words, the most common that fit the filter
/// (its `.slice(0, 1000)`).
const POOL_MAX_WORDS: usize = 1000;
/// Phonetic words pad the pool up to this many when fewer real words fit.
const POOL_MIN_WORDS: usize = 15;
/// Only dictionary words this long or longer, in characters, are used.
/// keybr.com drops the shorter ones (`word.length > 2`) when it builds its
/// dictionary.
const POOL_MIN_WORD_LEN: usize = 3;
/// Phonetic draws the padding may spend per word it still has to add. A
/// draw that dead-ends is skipped, so this bounds the work when the
/// letters can barely make a word.
const POOL_PAD_DRAWS_PER_WORD: usize = 4;

impl Default for WordGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl WordGenerator {
    pub fn new() -> Self {
        Self::for_language(Language::English)
    }

    /// A generator drawing on `language`'s phonetic model and word list.
    pub fn for_language(language: Language) -> Self {
        Self {
            rng: SimpleRng::new(),
            table: TransitionTable::for_language(language),
            dictionary: Dictionary::for_language(language),
            natural_words: true,
        }
    }

    #[cfg(test)]
    pub fn with_seed(seed: u32) -> Self {
        Self::with_seed_for(seed, Language::English)
    }

    #[cfg(test)]
    pub fn with_seed_for(seed: u32, language: Language) -> Self {
        Self {
            rng: SimpleRng::with_seed(seed),
            table: TransitionTable::for_language(language),
            dictionary: Dictionary::for_language(language),
            natural_words: true,
        }
    }

    /// Toggle the real-dictionary-word blend on or off. When off, the
    /// generator emits purely phonetic order-4 output.
    pub fn set_natural_words(&mut self, on: bool) {
        self.natural_words = on;
    }

    /// Current natural-words setting.
    #[allow(dead_code)]
    pub fn natural_words(&self) -> bool {
        self.natural_words
    }

    /// Read-only view of the embedded dictionary.
    ///
    /// `App::available_patterns` asks it which combination drills can
    /// actually be filled under the unlocked alphabet, so the settings
    /// screen never offers a drill that would generate no words. Shared
    /// rather than rebuilt because constructing a `Dictionary` parses the
    /// whole embedded wordlist.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Generate a single word respecting the letter filter, for a fragment
    /// that has no natural-words pool to draw from (see
    /// `generate_fragment`).
    ///
    /// A pattern focus draws a real word from the dictionary, even with
    /// `natural_words` off, because that toggle is about real words in
    /// ordinary practice and a combination drill has nothing to drill
    /// without them. It cannot fall back to the Markov model, which plants
    /// a single literal character at the front of a word and never
    /// constrains endings, so when the dictionary comes up empty the word
    /// is generated unfocused instead. Without a pattern the word is
    /// phonetic.
    fn next_word(&mut self, filter: &LetterFilter) -> String {
        if !is_pattern_focus(filter) {
            return self.phonetic_next_word(filter);
        }

        if let Some(word) = self.dictionary.next_word(filter, &mut self.rng) {
            return word.to_string();
        }

        // Dropping the focus for this word beats the degenerate filler,
        // which for a pattern would spell "tiontiontion".
        let unfocused = LetterFilter {
            allowed: filter.allowed.clone(),
            focused: None,
        };
        self.phonetic_next_word(&unfocused)
    }

    /// The words a natural-words lesson draws from, built the way
    /// keybr.com's `GuidedLesson` builds them: the first
    /// `POOL_MAX_WORDS` dictionary words of at least `POOL_MIN_WORD_LEN`
    /// characters that fit the filter, most common first, then phonetic
    /// words up to `POOL_MIN_WORDS`.
    ///
    /// The padding is what keeps an early lesson, whose few letters spell
    /// only a handful of real words, from repeating them. As on keybr.com,
    /// it may hold the same phonetic word twice. It never holds the
    /// degenerate filler (the focus key tripled): keybr.com pads with the
    /// partial word its model is left with after five dead ends, but our
    /// model retries differently (a known gap), so a draw that dead-ends
    /// is skipped instead, up to `POOL_PAD_DRAWS_PER_WORD` draws per
    /// missing word. Only a pool that would otherwise be empty gets the
    /// filler, where keybr.com pushes `"?"`.
    fn natural_word_pool(&mut self, filter: &LetterFilter) -> Vec<Cow<'static, str>> {
        // A `Key` focus outside the language's alphabet is no focus for
        // the padding either, as for `find`: planted in a phonetic word, a
        // space focus would split it in two.
        let filter = &LetterFilter {
            allowed: filter.allowed.clone(),
            focused: self.dictionary.enforceable_focus(filter),
        };
        let mut pool: Vec<Cow<'static, str>> = self
            .dictionary
            .find(filter, POOL_MIN_WORD_LEN, POOL_MAX_WORDS)
            .into_iter()
            .map(Cow::Borrowed)
            .collect();
        let mut draws = POOL_MIN_WORDS.saturating_sub(pool.len()) * POOL_PAD_DRAWS_PER_WORD;
        while pool.len() < POOL_MIN_WORDS && draws > 0 {
            draws -= 1;
            if let Some(word) = self.try_phonetic_word(filter) {
                pool.push(Cow::Owned(word));
            }
        }
        if pool.is_empty() {
            pool.push(Cow::Owned(filler_word(filter)));
        }
        pool
    }

    /// Generate a single word using only the order-4 phonetic Markov
    /// model, falling back to the degenerate filler when it cannot. It
    /// makes every word with `natural_words` off, stands in (unfocused)
    /// for a drill word the dictionary cannot supply, and is exposed
    /// directly for tests. The natural-words padding uses
    /// `try_phonetic_word` instead, which has no filler.
    ///
    /// Algorithm (matching keybr.com):
    /// 1. If there's a focused key, find a 1–3 char prefix containing it.
    /// 2. Otherwise seed with three spaces (word boundary history).
    /// 3. At each step: look up the order-4 segment for the last 3 chars,
    ///    filter to allowed alphabet indices.
    /// 4. If word shorter than `MIN_WORD_LEN`, drop space from candidates.
    /// 5. Boost space frequency by `1.3^word_length` (bias toward shorter words).
    /// 6. Weighted random select next char, advance history.
    /// 7. Max word length 10, retry up to 5 times on dead-ends.
    pub fn phonetic_next_word(&mut self, filter: &LetterFilter) -> String {
        self.try_phonetic_word(filter)
            .unwrap_or_else(|| filler_word(filter))
    }

    /// A phonetic word in up to `MAX_RETRIES` attempts, or `None` if every
    /// attempt dead-ends.
    fn try_phonetic_word(&mut self, filter: &LetterFilter) -> Option<String> {
        (0..MAX_RETRIES).find_map(|_| self.try_generate_word(filter))
    }

    fn try_generate_word(&mut self, filter: &LetterFilter) -> Option<String> {
        // Lengths are counted in characters, not bytes: Turkish letters
        // like 'ş' take two bytes in UTF-8.
        let mut word = String::with_capacity(MAX_WORD_LEN * 2);
        let mut word_len = 0usize;

        // Step 1: Seed the word. Default history is three spaces
        // (a fresh word boundary).
        let mut history: [usize; HISTORY_LEN] = [TransitionTable::SPACE; HISTORY_LEN];

        if let Some(FocusRule::Key(focused)) = filter.focused {
            let prefix = self.find_prefix_with_key(filter, focused);
            for c in prefix.chars() {
                word.push(c);
                word_len += 1;
            }
            // Pad/shift history with the prefix chars (right-aligned —
            // the most recent char of the prefix ends up at history[2]).
            for c in prefix.chars() {
                if let Some(idx) = self.table.char_to_idx(c) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
            }
        }

        // Step 3–7: Extend the word using order-4 sampling.
        loop {
            if word_len >= MAX_WORD_LEN {
                break;
            }

            match self.sample_next(&history, filter, word_len) {
                Some(' ') => break, // space = end of word
                Some(c) => {
                    word.push(c);
                    word_len += 1;
                    let idx = self
                        .table
                        .char_to_idx(c)
                        .expect("sampled char must be in alphabet");
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
                None => return None, // dead end, retry
            }
        }

        // Ensure the word satisfies the focus rule. A pattern can only get
        // here by luck, which is fine: the caller strips the focus before
        // using this path for one.
        if let Some(rule) = filter.focused {
            if !rule.matches(&word) {
                return None;
            }
        }

        if word_len < MIN_WORD_LEN {
            return None;
        }

        Some(word)
    }

    /// Find a 1–3 char prefix containing the focused key.
    ///
    /// We bias toward prefixes of length 3 because the order-4 model
    /// needs three characters of history before its conditioning is
    /// fully informative. The prefix is composed by walking the chain
    /// forward from the word-boundary history `[' ', ' ', ' ']`,
    /// weighting candidates that can reach the focused key.
    fn find_prefix_with_key(&mut self, filter: &LetterFilter, focused: char) -> String {
        // Strategy: pick a prefix length (1–3), then sample chars
        // forward respecting the filter, with a slight bias to make
        // sure the focused key appears.

        let prefix_len = 1 + (self.rng.next_bounded(3) as usize); // 1..=3
        let space = TransitionTable::SPACE;
        let mut history: [usize; HISTORY_LEN] = [space; HISTORY_LEN];
        let mut prefix = String::with_capacity(prefix_len);

        // We need the focused key to appear at least once in the prefix.
        // Plant it at a random position, then sample the rest.
        let focused_position = self.rng.next_bounded(prefix_len as u32) as usize;

        for pos in 0..prefix_len {
            if pos == focused_position {
                // Place the focused key here.
                prefix.push(focused);
                if let Some(idx) = self.table.char_to_idx(focused) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
                continue;
            }

            // Sample from the segment, refusing space (we don't want a
            // word boundary inside our prefix).
            let seg = self.table.segment(&history);
            let mut candidates: Vec<(char, u32)> = Vec::with_capacity(seg.len());
            for (i, &freq_val) in seg.iter().enumerate() {
                if freq_val == 0 || i == space {
                    continue;
                }
                let c = self.table.idx_to_char(i);
                if filter.is_allowed(c) {
                    candidates.push((c, freq_val as u32));
                }
            }
            if candidates.is_empty() {
                // Fall back to the focused key if we can't sample.
                prefix.push(focused);
                if let Some(idx) = self.table.char_to_idx(focused) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
            } else {
                let c = self.weighted_sample(&candidates);
                prefix.push(c);
                if let Some(idx) = self.table.char_to_idx(c) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
            }
        }

        prefix
    }

    /// Sample the next character given the order-4 history, respecting the filter.
    ///
    /// Returns `None` if no valid candidates exist (dead end).
    /// Returns `Some(' ')` to signal end of word.
    fn sample_next(
        &mut self,
        history: &[usize; HISTORY_LEN],
        filter: &LetterFilter,
        word_len: usize,
    ) -> Option<char> {
        let seg = self.table.segment(history);

        // Build filtered candidates: (char, adjusted_frequency).
        let mut candidates: Vec<(char, u32)> = Vec::with_capacity(seg.len());

        for (i, &freq_val) in seg.iter().enumerate() {
            let freq = freq_val as u32;
            if freq == 0 {
                continue;
            }
            let c = self.table.idx_to_char(i);

            if c == ' ' {
                // Step 4: If word is too short, skip space.
                if word_len < MIN_WORD_LEN {
                    continue;
                }
                // Step 5: Boost space frequency by 1.3^word_length.
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
    /// targeting approximately `target_len` characters (counted as
    /// characters, so a Turkish fragment is as long as an English one).
    ///
    /// With `natural_words` on and no pattern focus, the fragment's words
    /// come from `natural_word_pool`, built once here, each a uniform pick
    /// from it as in keybr.com's `randomWords`. Otherwise each word is
    /// drawn on its own by `next_word`.
    ///
    /// Duplicate words are rejected and redrawn, giving up after eleven
    /// tries so the loop always makes progress. A combination drill is
    /// offered from
    /// [`MIN_DRILL_WORDS`](crate::engine::filter::MIN_DRILL_WORDS) words
    /// up, so its pool runs dry partway through a long fragment and every
    /// remaining word costs eleven full dictionary scans to arrive at a
    /// word already used. Once a pattern draw has hit the bound, the pool
    /// is exhausted and repeating is the only way to fill the line, so
    /// rejection stops for the rest of the fragment.
    ///
    /// The bound is reachable on every branch, not only on drills.
    /// Measured at the default `fragment_length` of 100, the fresh
    /// profile's six-starter unfocused draw hits it, and so does `-TION`
    /// at nine unlocked letters. So `reject_duplicates = !pattern_focus`
    /// is not a claim that a `Key` focus or no focus never runs out of
    /// words. It is there so those two paths go on rejecting duplicates
    /// forever, exactly as they did before drills existed, and only a
    /// pattern focus is allowed to give up. Widening the scoping would
    /// silently change generated text for every letter-only user.
    pub fn generate_fragment(&mut self, filter: &LetterFilter, target_len: usize) -> String {
        let mut result = String::with_capacity(target_len * 2 + 20);
        let mut result_len = 0usize;
        let mut seen = std::collections::HashSet::new();
        let mut stuck_counter = 0;
        let pattern_focus = is_pattern_focus(filter);
        let mut reject_duplicates = true;
        let pool = (self.natural_words && !pattern_focus).then(|| self.natural_word_pool(filter));

        while result_len < target_len {
            let word = match &pool {
                Some(pool) => pool[self.rng.next_bounded(pool.len() as u32) as usize].to_string(),
                None => self.next_word(filter),
            };

            // Try to avoid duplicates, but don't loop forever.
            if reject_duplicates && seen.contains(&word) {
                stuck_counter += 1;
                if stuck_counter > 10 {
                    stuck_counter = 0;
                    // A small drill pool has nothing new left to offer.
                    reject_duplicates = !pattern_focus;
                } else {
                    continue;
                }
            }

            seen.insert(word.clone());

            if !result.is_empty() {
                result.push(' ');
                result_len += 1;
            }
            result_len += word.chars().count();
            result.push_str(&word);
        }

        result
    }
}

/// The degenerate word for when the phonetic model cannot finish one: the
/// focused key or first allowed key repeated.
fn filler_word(filter: &LetterFilter) -> String {
    // Only a `Key` focus has a character worth repeating here; a pattern
    // reaches this path only when called directly, and repeating it would
    // spell nonsense.
    let c = match filter.focused {
        Some(FocusRule::Key(c)) => c,
        _ => *filter.allowed.iter().next().unwrap_or(&'e'),
    };
    std::iter::repeat_n(c, MIN_WORD_LEN).collect()
}

/// True if `filter` drills a combination pattern rather than one letter.
fn is_pattern_focus(filter: &LetterFilter) -> bool {
    matches!(
        filter.focused,
        Some(FocusRule::Contains(_) | FocusRule::Suffix(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter(keys: &[char]) -> LetterFilter {
        LetterFilter::new(keys, None)
    }

    fn make_filter_focused(keys: &[char], focused: char) -> LetterFilter {
        LetterFilter::new(keys, Some(FocusRule::Key(focused)))
    }

    fn make_filter_rule(keys: &[char], rule: FocusRule) -> LetterFilter {
        LetterFilter::new(keys, Some(rule))
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
        // This constraint is a property of the phonetic order-4 model
        // (MIN_WORD_LEN = 3), not of real English (which includes "to",
        // "of", etc.). Disable the dictionary blend for this check.
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        gen.set_natural_words(false);
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
        // Same caveat: this is a phonetic-engine constraint.
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n', 's', 'r', 'h', 'l'];
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        gen.set_natural_words(false);
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

    #[test]
    fn a_lesson_nothing_fits_still_has_words_to_type() {
        // 'q' and 'z' together can't produce any real English word, and
        // the phonetic model cannot finish one either. With natural-words
        // on (default), the lesson must still be made of non-empty words
        // that respect the focused-key constraint.
        let keys = vec!['q', 'z'];
        let filter = make_filter_focused(&keys, 'q');
        let mut gen = WordGenerator::with_seed(123);
        assert!(gen.natural_words(), "natural_words should default to on");

        // Neither the dictionary nor the padding contributes, so the
        // whole pool is the degenerate filler.
        let text = gen.generate_fragment(&filter, 30);
        for w in text.split(' ') {
            assert!(!w.is_empty(), "generator should never return empty");
            assert!(w.contains('q'), "focused 'q' missing from '{w}'");
            for c in w.chars() {
                assert!(keys.contains(&c), "disallowed char '{c}' in '{w}'");
            }
        }
    }

    #[test]
    fn natural_words_toggle_disables_dictionary() {
        let keys: Vec<char> = ('a'..='z').collect();
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        gen.set_natural_words(false);
        assert!(!gen.natural_words());
        // With the dictionary disabled, this exercises only the phonetic
        // path, which spells words the word list does not have. With it
        // on, every word here would be in the list.
        let text = gen.generate_fragment(&filter, 200);
        assert!(!text.is_empty());
        let words: std::collections::HashSet<&str> = Language::English.wordlist().lines().collect();
        assert!(
            text.split_whitespace().any(|w| !words.contains(w)),
            "every word is a dictionary word: {text:?}"
        );
    }

    #[test]
    fn pattern_focus_draws_from_the_dictionary() {
        let keys: Vec<char> = ('a'..='z').collect();
        let filter = make_filter_rule(&keys, FocusRule::Contains("cr"));
        let mut gen = WordGenerator::with_seed(42);

        for _ in 0..20 {
            let w = gen.next_word(&filter);
            assert!(w.contains("cr"), "'cr' missing from '{w}'");
        }
    }

    #[test]
    fn pattern_focus_ignores_the_natural_words_toggle() {
        // A combination drill is meaningless without real words, so the
        // dictionary is consulted even with the blend switched off.
        let keys: Vec<char> = ('a'..='z').collect();
        let filter = make_filter_rule(&keys, FocusRule::Suffix("tion"));
        let mut gen = WordGenerator::with_seed(42);
        gen.set_natural_words(false);

        for _ in 0..20 {
            let w = gen.next_word(&filter);
            assert!(w.ends_with("tion"), "'{w}' does not end in 'tion'");
        }
    }

    /// The nine-letter alphabet (`UNLOCK_ORDER[..9]`) at which `-TION`
    /// has exactly eight matching words, the smallest pool
    /// `MIN_DRILL_WORDS` lets a drill be offered with. The fragment tests
    /// below drill this rather than a comfortable full-alphabet pattern,
    /// because a pool this size is what makes `generate_fragment` reach
    /// its duplicate-rejection bound and stop rejecting.
    const TINY_POOL_KEYS: [char; 9] = ['e', 'n', 'i', 'a', 'r', 'l', 't', 'o', 's'];

    /// Longest run of one word repeated back to back, which is what a
    /// drill that has given up on duplicate rejection risks reading like.
    fn longest_repeat_run(words: &[&str]) -> usize {
        let mut best = 0;
        let mut run = 0;
        let mut prev = "";
        for w in words {
            if *w == prev {
                run += 1;
            } else {
                run = 1;
                prev = w;
            }
            best = best.max(run);
        }
        best
    }

    #[test]
    fn the_tiny_drill_pool_is_still_eight_words() {
        // Guards the two fragment tests below. If the wordlist ever grows
        // a ninth "-tion" word spellable from these letters, they quietly
        // stop exercising a pool that runs dry, and the retry bound goes
        // back to having no coverage at all.
        let allowed: std::collections::HashSet<char> = TINY_POOL_KEYS.iter().copied().collect();
        let rule = FocusRule::Suffix("tion");
        let gen = WordGenerator::with_seed(1);
        let dict = gen.dictionary();
        assert!(
            dict.has_enough_matches(&rule, &allowed, 8),
            "-tion should have eight words here",
        );
        assert!(
            !dict.has_enough_matches(&rule, &allowed, 9),
            "-tion should have no more than eight words here",
        );
    }

    #[test]
    fn pattern_focus_fills_a_long_fragment_from_a_tiny_pool() {
        // 500 is the top of the fragment_length range the settings screen
        // allows, and eight words cannot fill it without repeating, so
        // this is the draw that hits the retry bound and switches
        // duplicate rejection off for the rest of the line. What must
        // hold afterwards: the fragment still reaches its target, is
        // never short, and every word still drills the pattern.
        let rule = FocusRule::Suffix("tion");
        let filter = make_filter_rule(&TINY_POOL_KEYS, rule);

        for seed in [1u32, 42, 7777] {
            let mut gen = WordGenerator::with_seed(seed);
            let text = gen.generate_fragment(&filter, 500);
            assert!(
                text.len() >= 500,
                "seed {seed}: fragment stopped at {} chars",
                text.len()
            );

            let words: Vec<&str> = text.split_whitespace().collect();
            for w in &words {
                assert!(rule.matches(w), "seed {seed}: '{w}' does not end in 'tion'");
                assert!(
                    w.chars().all(|c| TINY_POOL_KEYS.contains(&c)),
                    "seed {seed}: disallowed char in '{w}'",
                );
            }

            // Repetition is expected here and is the whole point of
            // giving up on rejection; degenerating into one word is not.
            // Both bounds are deliberately loose: they say "the pool is
            // still being drawn from", not "the sampler emits this exact
            // mix". A measured run at this length uses all eight words
            // with a longest back-to-back run of three.
            let distinct: std::collections::HashSet<&&str> = words.iter().collect();
            assert!(
                distinct.len() >= 4,
                "seed {seed}: only {} distinct words out of a pool of eight: {text:?}",
                distinct.len(),
            );
            let run = longest_repeat_run(&words);
            assert!(
                run <= 6,
                "seed {seed}: the same word {run} times in a row: {text:?}",
            );
        }
    }

    #[test]
    fn pattern_focus_fills_a_short_fragment_from_a_tiny_pool() {
        // 20 is the bottom of the same range. Two or three "-tion" words
        // clear it, so the retry bound is never reached: the check is
        // that the short end is not a special case, and in particular
        // that the target is met rather than undershot by the one word
        // that happened to overshoot it.
        let rule = FocusRule::Suffix("tion");
        let filter = make_filter_rule(&TINY_POOL_KEYS, rule);

        for seed in [1u32, 42, 7777] {
            let mut gen = WordGenerator::with_seed(seed);
            let text = gen.generate_fragment(&filter, 20);
            assert!(
                text.len() >= 20,
                "seed {seed}: fragment stopped at {} chars",
                text.len()
            );
            for w in text.split_whitespace() {
                assert!(rule.matches(w), "seed {seed}: '{w}' does not end in 'tion'");
            }
        }
    }

    #[test]
    fn unmatchable_pattern_falls_back_to_an_unfocused_word() {
        // '-ment' needs an 'm', which this alphabet lacks, so no dictionary
        // word can match. The generator must produce ordinary phonetic
        // words rather than a repeated-filler word.
        let keys: Vec<char> = ('a'..='z').filter(|&c| c != 'm').collect();
        let filter = make_filter_rule(&keys, FocusRule::Suffix("ment"));
        let mut gen = WordGenerator::with_seed(2024);

        let words: Vec<String> = (0..10).map(|_| gen.next_word(&filter)).collect();
        for w in &words {
            assert!(w.len() >= MIN_WORD_LEN, "filler-length word '{w}'");
            for c in w.chars() {
                assert!(keys.contains(&c), "disallowed char '{c}' in '{w}'");
            }
            // The degenerate filler is one character repeated; a real
            // generated word is not.
            let first = w.chars().next().unwrap();
            assert!(
                !w.chars().all(|c| c == first),
                "word '{w}' looks like the degenerate filler"
            );
        }
        let distinct: std::collections::HashSet<&String> = words.iter().collect();
        assert!(
            distinct.len() > 1,
            "fallback produced a constant word: {words:?}"
        );
    }

    #[test]
    fn non_alphabetic_key_focus_still_produces_a_clean_fragment() {
        // A `Key` focus outside the alphabet cannot be satisfied by any
        // dictionary word, and treating it as impossible rather than as
        // unfocused used to hand back empty words: the fragment came out
        // with leading and doubled spaces. It must read like ordinary
        // text. The full English alphabet fills the pool with real words;
        // the Greek starters fit one, so the rest is phonetic padding,
        // which must drop the focus too rather than plant a space.
        let english: Vec<char> = ('a'..='z').collect();
        let greek = unlocked(Language::Greek, 6);
        for (lang, keys) in [(Language::English, english), (Language::Greek, greek)] {
            let filter = make_filter_focused(&keys, ' ');
            let mut gen = WordGenerator::with_seed_for(4242, lang);
            let pool = gen.natural_word_pool(&filter);
            assert!(
                pool.iter().all(|w| !w.is_empty() && !w.contains(' ')),
                "{}: {pool:?}",
                lang.code()
            );

            for _ in 0..20 {
                let text = gen.generate_fragment(&filter, 100);
                assert!(!text.starts_with(' '), "leading space in: {text:?}");
                assert!(!text.ends_with(' '), "trailing space in: {text:?}");
                assert!(!text.contains("  "), "doubled space in: {text:?}");
                for word in text.split(' ') {
                    assert!(!word.is_empty(), "empty word in: {text:?}");
                    assert!(
                        word.chars().all(|c| keys.contains(&c)),
                        "disallowed char in '{word}'"
                    );
                }
            }
        }
    }

    #[test]
    fn key_focus_still_uses_the_degenerate_filler() {
        // The existing behaviour for a single key is untouched: 'q' and 'z'
        // make no real or phonetic word, so filler is all that is left.
        let keys = vec!['q', 'z'];
        let filter = make_filter_focused(&keys, 'q');
        let mut gen = WordGenerator::with_seed(123);
        let w = gen.phonetic_next_word(&filter);
        assert_eq!(w, "qqq");
    }

    /// Smoke test specified in the order-4 upgrade plan: generate 10
    /// words with the most common English keys, focused on 'e', and
    /// verify each word contains 'e', uses only allowed characters,
    /// and falls within the 3..=10 character word-length window.
    ///
    /// This drives the phonetic path directly so it remains a pure
    /// regression test for the order-4 model, independent of the
    /// dictionary blend layered on top.
    #[test]
    fn order4_smoke_test_focused_e() {
        let keys = vec!['e', 't', 'a', 'o', 'i', 'n'];
        let filter = make_filter_focused(&keys, 'e');
        let mut gen = WordGenerator::with_seed(0x5eed_b00b);

        let mut words = Vec::with_capacity(10);
        while words.len() < 10 {
            let w = gen.phonetic_next_word(&filter);
            words.push(w);
        }

        for word in &words {
            assert!(
                word.contains('e'),
                "focused key 'e' missing from word '{}'",
                word
            );
            assert!(
                word.len() >= 3 && word.len() <= 10,
                "word '{}' length {} outside 3..=10",
                word,
                word.len()
            );
            for ch in word.chars() {
                assert!(
                    keys.contains(&ch),
                    "word '{}' contains disallowed char '{}'",
                    word,
                    ch
                );
            }
        }
    }

    #[test]
    fn turkish_phonetic_words_use_turkish_letters_and_char_lengths() {
        // Starter set plus the next few letters, which already include
        // the dotless 'ı' and 'ü'.
        let keys: Vec<char> = Language::Turkish.unlock_order()[..12].to_vec();
        let filter = LetterFilter::new(&keys, Some(FocusRule::Key('ü')));
        let mut gen = WordGenerator::with_seed_for(42, Language::Turkish);
        gen.set_natural_words(false);
        let text = gen.generate_fragment(&filter, 100);
        assert!(text.chars().count() >= 100, "{text}");
        for word in text.split_whitespace() {
            assert!(
                word.contains('ü'),
                "focus 'ü' missing from '{word}' in {text}"
            );
            let n = word.chars().count();
            assert!((3..=10).contains(&n), "'{word}' has {n} chars");
            for c in word.chars() {
                assert!(keys.contains(&c), "disallowed '{c}' in '{word}'");
            }
        }
    }

    #[test]
    fn word_length_bounds_count_characters_not_bytes() {
        // 'ü' takes two bytes in UTF-8, and a focused word carries it in
        // its first three letters, so a length bound counted in bytes
        // would stop every such word at nine letters. The full alphabet
        // makes long words common enough to reach the bound.
        let keys: Vec<char> = Language::Turkish.letters().to_vec();
        let filter = LetterFilter::new(&keys, Some(FocusRule::Key('ü')));
        let mut gen = WordGenerator::with_seed_for(42, Language::Turkish);
        let lens: Vec<usize> = (0..1000)
            .map(|_| gen.phonetic_next_word(&filter).chars().count())
            .collect();
        assert!(
            lens.iter()
                .all(|n| (MIN_WORD_LEN..=MAX_WORD_LEN).contains(n)),
            "{lens:?}"
        );
        assert_eq!(lens.iter().max(), Some(&MAX_WORD_LEN), "{lens:?}");
    }

    #[test]
    fn turkish_natural_words_are_real_turkish_words() {
        let keys: Vec<char> = Language::Turkish.letters().to_vec();
        let filter = LetterFilter::new(&keys, None);
        let mut gen = WordGenerator::with_seed_for(7, Language::Turkish);
        let text = gen.generate_fragment(&filter, 200);
        let words: std::collections::HashSet<&str> = Language::Turkish.wordlist().lines().collect();
        for word in text.split_whitespace() {
            assert!(
                words.contains(word),
                "'{word}' is not in the Turkish word list"
            );
        }
    }

    /// The first `n` letters of `language`'s unlock order: the alphabet
    /// of a profile with `n` letters unlocked.
    fn unlocked(language: Language, n: usize) -> Vec<char> {
        language.unlock_order()[..n].to_vec()
    }

    #[test]
    fn greek_fresh_profile_lessons_are_not_a_few_words_repeated() {
        // A fresh Greek profile, 'ε' focused. Its six letters spell four
        // real words, but three are two letters long, and lessons made of
        // them alone read "τε εν ντε ει ει εν ντε". keybr.com skips the
        // short words and pads the one left with phonetic words.
        let lang = Language::Greek;
        let keys = unlocked(lang, 6);
        let filter = make_filter_focused(&keys, 'ε');
        let gen = WordGenerator::with_seed_for(1, lang);
        assert_eq!(
            gen.dictionary()
                .find(&filter, POOL_MIN_WORD_LEN, POOL_MAX_WORDS),
            ["ντε"]
        );

        for seed in [1u32, 42, 7777] {
            let mut gen = WordGenerator::with_seed_for(seed, lang);
            let text = gen.generate_fragment(&filter, 100);
            let words: Vec<&str> = text.split_whitespace().collect();
            let distinct: std::collections::HashSet<&&str> = words.iter().collect();
            assert!(distinct.len() > 4, "seed {seed}: {text:?}");
            for w in &words {
                assert!(w.contains('ε'), "seed {seed}: focus missing from '{w}'");
                assert!(
                    w.chars().all(|c| keys.contains(&c)),
                    "seed {seed}: disallowed char in '{w}'"
                );
                assert!(w.chars().count() >= 3, "seed {seed}: short word '{w}'");
            }
        }
    }

    #[test]
    fn turkish_lessons_at_eight_letters_are_not_ulu_repeated() {
        // With 'u' focused at eight letters, "ulu" is the only real word
        // that fits, and every lesson used to be "ulu ulu ulu". The
        // phonetic words padding it are what this checks for, not how
        // varied they are: at these letters the model mostly spells "unu".
        let lang = Language::Turkish;
        let keys = unlocked(lang, 8);
        let filter = make_filter_focused(&keys, 'u');
        let gen = WordGenerator::with_seed_for(1, lang);
        assert_eq!(gen.dictionary().find(&filter, 0, usize::MAX), ["ulu"]);

        for seed in [1u32, 42, 7777] {
            let mut gen = WordGenerator::with_seed_for(seed, lang);
            let text = gen.generate_fragment(&filter, 100);
            let words: Vec<&str> = text.split_whitespace().collect();
            assert!(words.iter().any(|w| *w != "ulu"), "seed {seed}: {text:?}");
            for w in &words {
                assert!(w.contains('u'), "seed {seed}: focus missing from '{w}'");
                assert!(
                    w.chars().all(|c| keys.contains(&c)),
                    "seed {seed}: disallowed char in '{w}'"
                );
            }
        }
    }

    #[test]
    fn a_lesson_draws_every_word_from_one_pool() {
        // keybr.com builds the pool once per lesson and picks from it.
        // Fourteen of the fifteen words here are phonetic, so a pool
        // rebuilt for each word would let in words from outside it. The
        // fragment builds its pool before it draws anything, so the same
        // seed builds the same pool.
        let lang = Language::Turkish;
        let filter = make_filter_focused(&unlocked(lang, 8), 'u');
        for seed in [1u32, 42, 7777] {
            let pool = WordGenerator::with_seed_for(seed, lang).natural_word_pool(&filter);
            assert_eq!(pool.len(), POOL_MIN_WORDS, "seed {seed}: {pool:?}");
            let mut gen = WordGenerator::with_seed_for(seed, lang);
            let text = gen.generate_fragment(&filter, 500);
            for w in text.split_whitespace() {
                assert!(
                    pool.iter().any(|p| p == w),
                    "seed {seed}: '{w}' is not in the lesson's pool {pool:?}"
                );
            }
        }
    }

    #[test]
    fn lessons_pick_uniformly_from_the_pool() {
        // keybr.com's `randomSample`: every word of the pool is equally
        // likely, the least common of the thousand as much as the most
        // common. Each half of the pool should take half the picks; a
        // biased pick would starve the second half.
        let keys = Language::English.letters().to_vec();
        let filter = make_filter(&keys);
        let mut gen = WordGenerator::with_seed(42);
        let pool = gen
            .dictionary()
            .find(&filter, POOL_MIN_WORD_LEN, POOL_MAX_WORDS);
        assert_eq!(pool.len(), POOL_MAX_WORDS);
        let rank: std::collections::HashMap<&str, usize> =
            pool.iter().enumerate().map(|(i, w)| (*w, i)).collect();

        let mut halves = [0usize; 2];
        for _ in 0..40 {
            let text = gen.generate_fragment(&filter, 500);
            for w in text.split_whitespace() {
                halves[rank[w] * 2 / pool.len()] += 1;
            }
        }
        let total = halves[0] + halves[1];
        // Over some three thousand picks, 45 to 55 percent is more than
        // five standard deviations either side of a fair half.
        assert!(
            (45..=55).contains(&(halves[1] * 100 / total)),
            "second half of the pool got {} of {total} picks",
            halves[1]
        );
    }

    #[test]
    fn lessons_skip_two_letter_words() {
        // These nine letters spell eleven two-letter words ("to", "it",
        // "is", "at" and more). The word list keeps them for combination
        // drills, but keybr.com never puts one in a lesson.
        let keys = unlocked(Language::English, 9);
        for focus in [None, Some(FocusRule::Key('s')), Some(FocusRule::Key('t'))] {
            let filter = LetterFilter::new(&keys, focus);
            let mut gen = WordGenerator::with_seed(42);
            let short: Vec<&str> = gen
                .dictionary()
                .find(&filter, 0, usize::MAX)
                .into_iter()
                .filter(|w| w.chars().count() < 3)
                .collect();
            assert!(!short.is_empty(), "{focus:?}: no two-letter word fits");

            let pool = gen.natural_word_pool(&filter);
            assert!(
                pool.iter().all(|w| w.chars().count() >= 3),
                "{focus:?}: short word in the pool {pool:?}"
            );
            for _ in 0..5 {
                let text = gen.generate_fragment(&filter, 200);
                for w in text.split_whitespace() {
                    assert!(w.chars().count() >= 3, "{focus:?}: '{w}' in {text:?}");
                }
            }
        }
    }

    #[test]
    fn fifteen_real_words_need_no_padding() {
        // With fifteen or more real words that fit, keybr.com adds no
        // phonetic words: the pool is the real words alone, and every word
        // of the lesson is real. A fresh English profile ('e' focused) has
        // 22; 'l' focused on the same letters has exactly fifteen, the
        // edge of the rule.
        let keys = unlocked(Language::English, 6);
        let words: std::collections::HashSet<&str> = Language::English.wordlist().lines().collect();
        for (focus, fit) in [('e', 22), ('l', POOL_MIN_WORDS)] {
            let filter = make_filter_focused(&keys, focus);
            let mut gen = WordGenerator::with_seed(42);
            let real = gen
                .dictionary()
                .find(&filter, POOL_MIN_WORD_LEN, POOL_MAX_WORDS);
            assert_eq!(real.len(), fit, "'{focus}': {real:?}");
            assert_eq!(gen.natural_word_pool(&filter), real, "'{focus}'");

            for seed in [1u32, 42, 7777] {
                let mut gen = WordGenerator::with_seed(seed);
                let text = gen.generate_fragment(&filter, 200);
                for w in text.split_whitespace() {
                    assert!(
                        words.contains(w),
                        "'{focus}', seed {seed}: '{w}' is not real"
                    );
                }
            }
        }
    }

    #[test]
    fn lessons_draw_from_the_thousand_most_common_words() {
        // keybr.com's `.slice(0, 1000)`. With every letter unlocked and no
        // focus, the pool is the first thousand words of three letters or
        // more in the word list, which is most common first. Two-letter
        // words are skipped, not counted, so the thousandth is past line
        // 1,000 of the file.
        let keys = Language::English.letters().to_vec();
        let filter = make_filter(&keys);
        let top: Vec<&str> = Language::English
            .wordlist()
            .lines()
            .filter(|w| w.chars().count() >= 3)
            .take(1000)
            .collect();
        let mut gen = WordGenerator::with_seed(42);
        assert_eq!(gen.natural_word_pool(&filter), top);

        let top: std::collections::HashSet<&str> = top.into_iter().collect();
        for _ in 0..10 {
            let text = gen.generate_fragment(&filter, 500);
            for w in text.split_whitespace() {
                assert!(top.contains(w), "'{w}' is not among the 1,000 most common");
            }
        }
    }

    #[test]
    fn padding_never_uses_the_degenerate_filler() {
        // When the phonetic model dead-ends, `phonetic_next_word` falls
        // back to the focus key tripled. keybr.com never shows that, so
        // the padding skips the draw instead. A fresh Ukrainian profile
        // ('и' focused, nine real words) and a fresh Greek one ('ε', one
        // real word) dead-end often enough that filler padding put "иии"
        // in most Ukrainian lessons.
        for (lang, focus) in [(Language::Ukrainian, 'и'), (Language::Greek, 'ε')] {
            let keys = unlocked(lang, 6);
            let filter = make_filter_focused(&keys, focus);
            let filler: String = [focus; 3].iter().collect();
            let mut gen = WordGenerator::with_seed_for(31, lang);
            for _ in 0..100 {
                let pool = gen.natural_word_pool(&filter);
                assert!(
                    pool.iter().all(|w| *w != filler),
                    "{}: filler in the pool {pool:?}",
                    lang.code()
                );
                let text = gen.generate_fragment(&filter, 100);
                assert!(
                    text.split_whitespace().all(|w| w != filler),
                    "{}: filler in {text:?}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn a_pool_nothing_fits_holds_only_the_filler() {
        // No real word fits 'q' and 'z', and the phonetic model cannot
        // finish one either. keybr.com pushes "?" so the lesson has
        // something to type; the filler is ours, and it is all there is.
        let filter = make_filter_focused(&['q', 'z'], 'q');
        let mut gen = WordGenerator::with_seed(123);
        assert_eq!(gen.natural_word_pool(&filter), ["qqq"]);
    }
}
