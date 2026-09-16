use super::dictionary::Dictionary;
use super::filter::{FocusRule, LetterFilter};
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
/// that reads as plausible English at filter boundaries.
pub struct WordGenerator {
    rng: SimpleRng,
    table: TransitionTable,
    dictionary: Dictionary,
    /// When true, prefer real dictionary words and fall back to the
    /// phonetic model only when no dictionary word matches the filter.
    natural_words: bool,
}

const MAX_WORD_LEN: usize = 10;
const MIN_WORD_LEN: usize = 3;
const MAX_RETRIES: usize = 5;
const SPACE_BOOST_BASE: f64 = 1.3;

/// History length is `order - 1`. For the order-4 English model this is 3.
const HISTORY_LEN: usize = 3;

impl Default for WordGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl WordGenerator {
    pub fn new() -> Self {
        Self {
            rng: SimpleRng::new(),
            table: TransitionTable::from_embedded(),
            dictionary: Dictionary::from_embedded(),
            natural_words: true,
        }
    }

    #[cfg(test)]
    pub fn with_seed(seed: u32) -> Self {
        Self {
            rng: SimpleRng::with_seed(seed),
            table: TransitionTable::from_embedded(),
            dictionary: Dictionary::from_embedded(),
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

    /// Generate a single word respecting the letter filter.
    ///
    /// If `natural_words` is enabled, first try to draw a real English
    /// word from the embedded dictionary that satisfies the filter. If
    /// no such word exists (e.g. the active letter set is too restrictive
    /// for any real word — typical early in the curriculum), fall back to
    /// the phonetic order-4 model so the user still has plausible-looking
    /// practice text.
    ///
    /// A pattern focus is the exception on both counts. It reads the
    /// dictionary even with `natural_words` off, because that toggle is
    /// about blending real words into ordinary practice and a combination
    /// drill has nothing to drill without them. And it cannot fall back to
    /// the Markov model, which plants a single literal character at the
    /// front of a word and never constrains endings, so when the
    /// dictionary comes up empty the word is generated unfocused instead.
    pub fn next_word(&mut self, filter: &LetterFilter) -> String {
        let pattern_focus = matches!(
            filter.focused,
            Some(FocusRule::Contains(_) | FocusRule::Suffix(_))
        );

        if self.natural_words || pattern_focus {
            if let Some(word) = self.dictionary.next_word(filter, &mut self.rng) {
                return word.to_string();
            }
        }

        if pattern_focus {
            // Dropping the focus for this word beats the degenerate filler,
            // which for a pattern would spell "tiontiontion".
            let unfocused = LetterFilter {
                allowed: filter.allowed.clone(),
                focused: None,
            };
            return self.phonetic_next_word(&unfocused);
        }

        self.phonetic_next_word(filter)
    }

    /// Generate a single word using only the order-4 phonetic Markov
    /// model. Used as a fallback when no dictionary word matches the
    /// filter, and exposed directly for tests.
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
        for _ in 0..MAX_RETRIES {
            if let Some(word) = self.try_generate_word(filter) {
                return word;
            }
        }
        // Fallback: return a single focused key or first allowed key repeated.
        // Only a `Key` focus has a character worth repeating here; a pattern
        // reaches this path only when called directly, and repeating it
        // would spell nonsense.
        let c = match filter.focused {
            Some(FocusRule::Key(c)) => c,
            _ => *filter.allowed.iter().next().unwrap_or(&'e'),
        };
        // Note: `repeat_n` requires Rust 1.82+; MSRV here is 1.75.
        #[allow(clippy::manual_repeat_n)]
        std::iter::repeat(c).take(MIN_WORD_LEN).collect()
    }

    fn try_generate_word(&mut self, filter: &LetterFilter) -> Option<String> {
        let mut word = String::with_capacity(MAX_WORD_LEN);

        // Step 1: Seed the word. Default history is three spaces
        // (a fresh word boundary).
        let space = TransitionTable::char_to_idx(' ').unwrap();
        let mut history: [usize; HISTORY_LEN] = [space; HISTORY_LEN];

        if let Some(FocusRule::Key(focused)) = filter.focused {
            let prefix = self.find_prefix_with_key(filter, focused);
            for c in prefix.chars() {
                word.push(c);
            }
            // Pad/shift history with the prefix chars (right-aligned —
            // the most recent char of the prefix ends up at history[2]).
            for c in prefix.chars() {
                if let Some(idx) = TransitionTable::char_to_idx(c) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
            }
        }

        // Step 3–7: Extend the word using order-4 sampling.
        loop {
            if word.len() >= MAX_WORD_LEN {
                break;
            }

            match self.sample_next(&history, filter, word.len()) {
                Some(' ') => break, // space = end of word
                Some(c) => {
                    word.push(c);
                    let idx =
                        TransitionTable::char_to_idx(c).expect("sampled char must be in alphabet");
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

        if word.len() < MIN_WORD_LEN {
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
        let space = TransitionTable::char_to_idx(' ').unwrap();
        let mut history: [usize; HISTORY_LEN] = [space; HISTORY_LEN];
        let mut prefix = String::with_capacity(prefix_len);

        // We need the focused key to appear at least once in the prefix.
        // Plant it at a random position, then sample the rest.
        let focused_position = self.rng.next_bounded(prefix_len as u32) as usize;

        for pos in 0..prefix_len {
            if pos == focused_position {
                // Place the focused key here.
                prefix.push(focused);
                if let Some(idx) = TransitionTable::char_to_idx(focused) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
                continue;
            }

            // Sample from the segment, refusing space (we don't want a
            // word boundary inside our prefix).
            let seg = self.table.segment(&history);
            let mut candidates: Vec<(char, u32)> = Vec::with_capacity(26);
            for (i, &freq_val) in seg.iter().enumerate() {
                if freq_val == 0 || i == space {
                    continue;
                }
                let c = TransitionTable::idx_to_char(i);
                if filter.is_allowed(c) {
                    candidates.push((c, freq_val as u32));
                }
            }
            if candidates.is_empty() {
                // Fall back to the focused key if we can't sample.
                prefix.push(focused);
                if let Some(idx) = TransitionTable::char_to_idx(focused) {
                    history.copy_within(1.., 0);
                    history[HISTORY_LEN - 1] = idx;
                }
            } else {
                let c = self.weighted_sample(&candidates);
                prefix.push(c);
                if let Some(idx) = TransitionTable::char_to_idx(c) {
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
        let mut candidates: Vec<(char, u32)> = Vec::with_capacity(27);

        for (i, &freq_val) in seg.iter().enumerate() {
            let freq = freq_val as u32;
            if freq == 0 {
                continue;
            }
            let c = TransitionTable::idx_to_char(i);

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
    /// targeting approximately `target_len` characters.
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
    /// words. It is there to keep those two paths byte-for-byte identical
    /// to the text they generated before drills existed: they go on
    /// rejecting duplicates forever, exactly as they always did, and only
    /// a pattern focus is allowed to give up. Widening the scoping would
    /// silently change generated text for every letter-only user.
    pub fn generate_fragment(&mut self, filter: &LetterFilter, target_len: usize) -> String {
        let mut result = String::with_capacity(target_len + 20);
        let mut seen = std::collections::HashSet::new();
        let mut stuck_counter = 0;
        let pattern_focus = matches!(
            filter.focused,
            Some(FocusRule::Contains(_) | FocusRule::Suffix(_))
        );
        let mut reject_duplicates = true;

        while result.len() < target_len {
            let word = self.next_word(filter);

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
    fn dictionary_blend_falls_back_to_phonetic() {
        // 'q' and 'z' together can't produce any real English word.
        // With natural-words on (default), the generator must still
        // return a non-empty word via the phonetic fallback path —
        // and it must respect the focused-key constraint.
        let keys = vec!['q', 'z'];
        let filter = make_filter_focused(&keys, 'q');
        let mut gen = WordGenerator::with_seed(123);
        assert!(gen.natural_words(), "natural_words should default to on");

        // Generate a handful — the dictionary path will yield None each
        // time and the phonetic path will produce filler words.
        for _ in 0..5 {
            let w = gen.next_word(&filter);
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
        // With dictionary disabled, this exercises only the phonetic path.
        let w = gen.next_word(&filter);
        assert!(!w.is_empty());
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
        // A `Key` focus outside a-z cannot be satisfied by any dictionary
        // word, and treating it as impossible rather than as unfocused
        // used to hand back empty words: the fragment came out with
        // leading and doubled spaces. It must read like ordinary text.
        let keys: Vec<char> = ('a'..='z').collect();
        let filter = make_filter_focused(&keys, ' ');
        let mut gen = WordGenerator::with_seed(4242);
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
}
