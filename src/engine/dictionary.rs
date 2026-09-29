use std::collections::HashSet;

use crate::engine::filter::{FocusRule, LetterFilter};
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
/// distinct letters it contains. With a focus rule, the candidate pool is
/// the bucket for its first letter (every word satisfying the rule must
/// contain that letter); without one, all words are candidates. The bucket
/// only narrows the pool, so the rule is still verified per word at sample
/// time. At ~10K words a per-character allowed-set check is plenty fast, no
/// need for a bitmap index.
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
    /// - satisfies the filter's focus rule (if any), and
    /// - uses only characters in `filter.allowed` (plus space — but words
    ///   never contain space, so this is moot for individual words).
    ///
    /// Returns `None` if no word satisfies the filter. Callers should
    /// fall back to phonetic generation in that case.
    pub fn next_word(&self, filter: &LetterFilter, rng: &mut SimpleRng) -> Option<&'static str> {
        // 1. Reduce the focus to the rule this dictionary can actually
        //    enforce. See `enforceable_rule`.
        let rule = filter.focused.and_then(enforceable_rule);

        // 2. Narrow the candidate pool to the bucket for the rule's first
        //    letter. Every word satisfying the rule contains it, so this
        //    loses no candidates, and for a multi-char rule it is only a
        //    pre-narrowing: `sample_filtered` still checks the rule itself.
        let pool: Option<&[u32]> = match rule.and_then(bucket_letter) {
            Some(c) => {
                let bucket = &self.by_letter[(c as u8 - b'a') as usize];
                if bucket.is_empty() {
                    return None;
                }
                Some(bucket)
            }
            // Nothing to enforce, or nothing to index on: consider every
            // word and let the allowed-set check do the filtering.
            None => None,
        };

        self.sample_filtered(filter, rule, rng, pool)
    }

    /// True if at least `min` distinct words satisfy `rule` using only
    /// `allowed` characters.
    ///
    /// This gates whether a combination drill is offered at all, so it is
    /// an exact answer rather than a sampled one: a full scan that returns
    /// as soon as the count is reached, which is microseconds at ~10K
    /// words. The wordlist holds no duplicates, so counting entries counts
    /// distinct words.
    ///
    /// A drill needs a pool, not a single hit: see
    /// [`crate::engine::filter::MIN_DRILL_WORDS`] for why `min` is what it
    /// is.
    pub fn has_enough_matches(
        &self,
        rule: &FocusRule,
        allowed: &HashSet<char>,
        min: usize,
    ) -> bool {
        if min == 0 {
            return true;
        }
        let mut found = 0usize;
        for w in &self.all_words {
            // Words are ASCII lowercase a-z by construction, so the space
            // exemption in `LetterFilter::is_allowed` never applies here.
            if rule.matches(w) && w.chars().all(|c| allowed.contains(&c)) {
                found += 1;
                if found >= min {
                    return true;
                }
            }
        }
        false
    }

    /// Sample a random word from either a focused pool of indices or all
    /// words, rejecting those that fail the filter's focus rule or whose
    /// characters aren't all in `filter.allowed`.
    ///
    /// Uses bounded rejection sampling: tries up to N random picks before
    /// scanning the full pool for any valid word. With ~10K words and a
    /// reasonable allowed-set this almost always succeeds on the first pick.
    fn sample_filtered(
        &self,
        filter: &LetterFilter,
        rule: Option<FocusRule>,
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
            if word_matches_filter(word, filter, rule) {
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
                    if word_matches_filter(w, filter, rule) {
                        valid.push(w);
                    }
                }
            }
            None => {
                for &w in &self.all_words {
                    if word_matches_filter(w, filter, rule) {
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

/// The part of a focus rule the dictionary enforces, which for a `Key`
/// outside a-z is nothing at all.
///
/// Every word here is ASCII lowercase a-z, so a `Key(' ')` focus (the
/// scheduler's key set is a-z, but the type admits any char) matches no
/// word whatsoever. Enforcing it would reject the entire wordlist and
/// leave the caller with no word to place; treating it as unfocused
/// returns a real word, which is what this dictionary did before
/// combination drills existed. Patterns always start a-z, so dropping the
/// rule here never weakens a drill.
#[inline]
fn enforceable_rule(rule: FocusRule) -> Option<FocusRule> {
    match rule {
        FocusRule::Key(c) if !c.is_ascii_lowercase() => None,
        other => Some(other),
    }
}

/// The a-z letter whose bucket can hold every word satisfying `rule`: the
/// key itself, or the first letter of a pattern. `None` means there is no
/// such bucket and the whole wordlist has to be considered.
#[inline]
fn bucket_letter(rule: FocusRule) -> Option<char> {
    let first = match rule {
        FocusRule::Key(c) => c,
        FocusRule::Contains(p) | FocusRule::Suffix(p) => p.chars().next()?,
    };
    first.is_ascii_lowercase().then_some(first)
}

/// True if `word` satisfies `rule` and every character in it is allowed.
/// Words are ASCII lowercase a–z by construction, so we don't need to
/// consider space or uppercase.
///
/// `rule` is the enforceable rule from `next_word`, not `filter.focused`
/// verbatim. The check is load-bearing rather than a belt-and-braces
/// repeat of the bucket choice: a bucket only proves the word contains the
/// rule's first letter, which for a pattern says nothing about order or
/// position.
#[inline]
fn word_matches_filter(word: &str, filter: &LetterFilter, rule: Option<FocusRule>) -> bool {
    word.chars().all(|c| filter.is_allowed(c)) && rule.is_none_or(|rule| rule.matches(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::filter::MIN_DRILL_WORDS;

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
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Key('e')));
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
        let filter = LetterFilter::new(&['a', 'b'], Some(FocusRule::Key('q')));
        let mut rng = SimpleRng::with_seed(42);
        assert!(dict.next_word(&filter, &mut rng).is_none());
    }

    #[test]
    fn next_word_respects_contains_rule() {
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = ('a'..='z').collect();
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Contains("cr")));
        let mut rng = SimpleRng::with_seed(42);

        for _ in 0..100 {
            let word = dict
                .next_word(&filter, &mut rng)
                .expect("plenty of words contain 'cr'");
            assert!(word.contains("cr"), "'cr' missing from word '{word}'");
        }
    }

    #[test]
    fn next_word_respects_suffix_rule() {
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = ('a'..='z').collect();
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Suffix("ing")));
        let mut rng = SimpleRng::with_seed(7);

        for _ in 0..100 {
            let word = dict
                .next_word(&filter, &mut rng)
                .expect("plenty of words end in 'ing'");
            assert!(word.ends_with("ing"), "word '{word}' does not end in 'ing'");
        }
    }

    #[test]
    fn pattern_rule_also_respects_allowed_set() {
        let dict = Dictionary::from_embedded();
        // Enough letters for real words, but no 'y', 'w' or 'k'.
        let allowed: Vec<char> = vec![
            'a', 'b', 'c', 'd', 'e', 'g', 'i', 'l', 'm', 'n', 'o', 'p', 'r', 's', 't', 'u',
        ];
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Suffix("ing")));
        let mut rng = SimpleRng::with_seed(99);

        for _ in 0..100 {
            let word = dict
                .next_word(&filter, &mut rng)
                .expect("restricted set still has -ing words");
            assert!(word.ends_with("ing"), "word '{word}' does not end in 'ing'");
            for c in word.chars() {
                assert!(
                    allowed.contains(&c),
                    "word '{word}' contains disallowed char '{c}'",
                );
            }
        }
    }

    #[test]
    fn pattern_rule_returns_none_when_a_letter_is_locked() {
        let dict = Dictionary::from_embedded();
        // Every 'cr' word needs a 'c'; without it there is nothing to draw.
        let allowed: Vec<char> = ('a'..='z').filter(|&c| c != 'c').collect();
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Contains("cr")));
        let mut rng = SimpleRng::with_seed(42);
        assert!(dict.next_word(&filter, &mut rng).is_none());
    }

    #[test]
    fn has_enough_matches_is_true_for_reachable_patterns() {
        let dict = Dictionary::from_embedded();
        let all: HashSet<char> = ('a'..='z').collect();
        assert!(dict.has_enough_matches(&FocusRule::Contains("cr"), &all, MIN_DRILL_WORDS));
        assert!(dict.has_enough_matches(&FocusRule::Suffix("tion"), &all, MIN_DRILL_WORDS));
        assert!(dict.has_enough_matches(&FocusRule::Key('e'), &all, MIN_DRILL_WORDS));
    }

    #[test]
    fn has_enough_matches_is_false_when_letters_are_missing() {
        let dict = Dictionary::from_embedded();
        // 'ment' needs an 'm'.
        let no_m: HashSet<char> = ('a'..='z').filter(|&c| c != 'm').collect();
        assert!(!dict.has_enough_matches(&FocusRule::Suffix("ment"), &no_m, MIN_DRILL_WORDS));
        // A tiny starter alphabet reaches no pattern at all.
        let starter: HashSet<char> = vec!['e', 't', 'a', 'o'].into_iter().collect();
        assert!(!dict.has_enough_matches(&FocusRule::Contains("cr"), &starter, MIN_DRILL_WORDS));
        assert!(!dict.has_enough_matches(&FocusRule::Suffix("ing"), &starter, MIN_DRILL_WORDS));
    }

    #[test]
    fn has_enough_matches_counts_rather_than_stopping_at_one() {
        let dict = Dictionary::from_embedded();
        // The starter alphabet plus enough letters to spell exactly one
        // '-ous' word, 'serious'. One word is not a drill, so the gate
        // that mattered at base (min 1) says yes and the real gate says no.
        let thin: HashSet<char> = "enialrtosu".chars().collect();
        let rule = FocusRule::Suffix("ous");
        assert!(
            dict.has_enough_matches(&rule, &thin, 1),
            "'serious' is spellable here"
        );
        assert!(
            !dict.has_enough_matches(&rule, &thin, MIN_DRILL_WORDS),
            "a one-word pool must not clear the drill threshold"
        );
    }

    #[test]
    fn has_enough_matches_returns_as_soon_as_min_is_reached() {
        let dict = Dictionary::from_embedded();
        let all: HashSet<char> = ('a'..='z').collect();
        // A min of zero is vacuously satisfied, and a min beyond the whole
        // wordlist can never be.
        assert!(dict.has_enough_matches(&FocusRule::Suffix("tion"), &all, 0));
        assert!(!dict.has_enough_matches(&FocusRule::Suffix("tion"), &all, dict.len() + 1));
    }

    #[test]
    fn has_enough_matches_agrees_with_sampling() {
        // The availability gate and the sampler must never disagree, or the
        // UI offers a drill that yields no words. The gate is strictly the
        // stronger test: whatever it accepts, the sampler can fill.
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = ('a'..='z').filter(|&c| c != 'c').collect();
        let set: HashSet<char> = allowed.iter().copied().collect();
        let mut rng = SimpleRng::with_seed(5);
        for rule in crate::engine::filter::FOCUS_PATTERNS {
            let filter = LetterFilter::new(&allowed, Some(*rule));
            assert_eq!(
                dict.has_enough_matches(rule, &set, 1),
                dict.next_word(&filter, &mut rng).is_some(),
                "gate and sampler disagree on {}",
                rule.config_value()
            );
            if dict.has_enough_matches(rule, &set, MIN_DRILL_WORDS) {
                assert!(
                    dict.next_word(&filter, &mut rng).is_some(),
                    "offered drill {} yields no word",
                    rule.config_value()
                );
            }
        }
    }

    #[test]
    fn non_alphabetic_key_focus_is_treated_as_unfocused() {
        // A `Key` focus with no a-z letter cannot be satisfied by any
        // word here, so it places no constraint rather than rejecting the
        // whole dictionary. This is the pre-drills behaviour and the
        // generator's fallback path depends on it.
        let dict = Dictionary::from_embedded();
        let allowed: Vec<char> = ('a'..='z').collect();
        let filter = LetterFilter::new(&allowed, Some(FocusRule::Key(' ')));
        let mut rng = SimpleRng::with_seed(11);
        for _ in 0..50 {
            let word = dict
                .next_word(&filter, &mut rng)
                .expect("a space focus must still yield a real word");
            assert!(!word.is_empty());
            assert!(word.bytes().all(|b| b.is_ascii_lowercase()));
        }

        // The allowed set is still honoured: the focus is dropped, not the
        // filter.
        let narrow: Vec<char> = vec!['a', 'e', 't', 'o', 'n'];
        let filter = LetterFilter::new(&narrow, Some(FocusRule::Key(' ')));
        for _ in 0..50 {
            if let Some(word) = dict.next_word(&filter, &mut rng) {
                assert!(word.chars().all(|c| narrow.contains(&c)), "{word}");
            }
        }
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
