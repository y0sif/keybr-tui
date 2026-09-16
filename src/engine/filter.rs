use std::collections::HashSet;

/// What every generated word must satisfy.
///
/// `Key` is the adaptive curriculum's focus, the single weakest letter the
/// user is practicing. The pattern variants drive combination drills, where
/// the point is the reach between two letters, or the shape of a word
/// ending, rather than any one letter on its own. That is why they match
/// against the whole word instead of its character set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRule {
    /// The character must appear somewhere in the word.
    Key(char),
    /// The substring must appear somewhere in the word.
    Contains(&'static str),
    /// The word must end with this substring.
    Suffix(&'static str),
}

/// How many distinct dictionary words a combination drill needs before it
/// is worth offering.
///
/// One matching word is not a drill, it is a memorisation exercise: a
/// lesson is filled by repeating whatever the dictionary has, so a pattern
/// backed by a single word spells that word forty times. Eight is the
/// smallest pool that reads as practice rather than as a stuck generator.
/// Measured against the unlock order, it removes every degenerate drill,
/// puts the first offer (`-ER`, twelve words) at the point where `t`
/// unlocks, and has all sixteen drills reachable by twenty-one letters.
///
/// It does not keep `generate_fragment`'s duplicate-rejection loop off
/// its retry path, and nothing does: a pool this small runs dry inside a
/// long fragment, and even the thousands-of-words unfocused draw reaches
/// the retry bound at the default fragment length. That is why the
/// generator stops rejecting duplicates once a pattern draw has hit the
/// bound, rather than relying on a pool size that never hits it.
pub const MIN_DRILL_WORDS: usize = 8;

/// The combination drills the user can pick from.
///
/// `FocusRule::from_config` only ever returns a rule from this list (or a
/// bare letter), so this is the single place to add or retire a drill.
pub const FOCUS_PATTERNS: &[FocusRule] = &[
    // Consonant-cluster reaches, the "scissors" the issue asks for.
    FocusRule::Contains("cr"),
    FocusRule::Contains("pl"),
    FocusRule::Contains("br"),
    FocusRule::Contains("tr"),
    FocusRule::Contains("gr"),
    FocusRule::Contains("fr"),
    FocusRule::Contains("bl"),
    FocusRule::Contains("cl"),
    // Word endings.
    FocusRule::Suffix("ing"),
    FocusRule::Suffix("ed"),
    FocusRule::Suffix("er"),
    FocusRule::Suffix("ly"),
    FocusRule::Suffix("tion"),
    FocusRule::Suffix("ment"),
    FocusRule::Suffix("able"),
    FocusRule::Suffix("ous"),
];

impl FocusRule {
    /// True if `word` satisfies this rule.
    pub fn matches(&self, word: &str) -> bool {
        match self {
            FocusRule::Key(c) => word.contains(*c),
            FocusRule::Contains(p) => word.contains(p),
            FocusRule::Suffix(p) => word.ends_with(p),
        }
    }

    /// The distinct letters this rule involves, in the order they appear.
    ///
    /// The key bar highlights these tiles, so `Suffix("tion")` lights up
    /// four keys where `Key('e')` lights up one.
    pub fn letters(&self) -> Vec<char> {
        let mut out = Vec::new();
        match self {
            FocusRule::Key(c) => out.push(*c),
            FocusRule::Contains(p) | FocusRule::Suffix(p) => {
                for c in p.chars() {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    /// Display form: `"E"`, `"CR"`, `"-TION"`. The leading dash is how the
    /// UI says "word ending" without spending a word on it.
    pub fn label(&self) -> String {
        match self {
            FocusRule::Key(c) => c.to_uppercase().to_string(),
            FocusRule::Contains(p) => p.to_uppercase(),
            FocusRule::Suffix(p) => format!("-{}", p.to_uppercase()),
        }
    }

    /// Persistence form, the lowercase mirror of `label`: `"e"`, `"cr"`,
    /// `"-tion"`.
    pub fn config_value(&self) -> String {
        match self {
            FocusRule::Key(c) => c.to_lowercase().to_string(),
            FocusRule::Contains(p) => (*p).to_string(),
            FocusRule::Suffix(p) => format!("-{p}"),
        }
    }

    /// Parse a `config_value` back into a rule.
    ///
    /// The config file is hand-edited, so this is deliberately strict: the
    /// only accepted values are one `a-z` letter and the exact lowercase
    /// spelling of a preset in `FOCUS_PATTERNS`. Uppercase, stray
    /// whitespace and retired patterns all read as `None`, which the
    /// caller treats as "no pin" rather than as an error.
    pub fn from_config(s: &str) -> Option<Self> {
        if let Some(suffix) = s.strip_prefix('-') {
            return FOCUS_PATTERNS
                .iter()
                .copied()
                .find(|r| matches!(r, FocusRule::Suffix(p) if *p == suffix));
        }

        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return c.is_ascii_lowercase().then_some(FocusRule::Key(c));
        }

        FOCUS_PATTERNS
            .iter()
            .copied()
            .find(|r| matches!(r, FocusRule::Contains(p) if *p == s))
    }
}

/// Controls which characters the word generator is allowed to use.
///
/// The `focused` rule, if set, MUST be satisfied by every generated word:
/// the weakest key for the adaptive curriculum, or a combination pattern
/// when the user is drilling one.
pub struct LetterFilter {
    /// Set of characters allowed in generation.
    pub allowed: HashSet<char>,
    /// The rule every word MUST satisfy.
    pub focused: Option<FocusRule>,
}

impl LetterFilter {
    /// Create a filter from a slice of allowed characters and an optional focus rule.
    pub fn new(allowed: &[char], focused: Option<FocusRule>) -> Self {
        Self {
            allowed: allowed.iter().copied().collect(),
            focused,
        }
    }

    /// Check whether a character is allowed by this filter.
    #[inline]
    pub fn is_allowed(&self, c: char) -> bool {
        c == ' ' || self.allowed.contains(&c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_is_always_allowed() {
        let filter = LetterFilter::new(&['a', 'b'], None);
        assert!(filter.is_allowed(' '));
    }

    #[test]
    fn allowed_chars_pass() {
        let filter = LetterFilter::new(&['e', 't', 'a'], None);
        assert!(filter.is_allowed('e'));
        assert!(filter.is_allowed('t'));
        assert!(!filter.is_allowed('z'));
    }

    #[test]
    fn focused_is_stored() {
        let filter = LetterFilter::new(&['e', 't'], Some(FocusRule::Key('t')));
        assert_eq!(filter.focused, Some(FocusRule::Key('t')));
    }

    #[test]
    fn key_matches_anywhere_in_word() {
        let rule = FocusRule::Key('e');
        assert!(rule.matches("test"));
        assert!(rule.matches("eat"));
        assert!(!rule.matches("cat"));
    }

    #[test]
    fn contains_matches_anywhere_in_word() {
        let rule = FocusRule::Contains("cr");
        assert!(rule.matches("create"));
        assert!(rule.matches("secret"));
        assert!(rule.matches("acre"));
        assert!(!rule.matches("car"));
        assert!(!rule.matches("rc"));
    }

    #[test]
    fn suffix_matches_only_at_the_end() {
        let rule = FocusRule::Suffix("ing");
        assert!(rule.matches("typing"));
        assert!(rule.matches("ing"));
        assert!(!rule.matches("ingot"));
        assert!(!rule.matches("sign"));
    }

    #[test]
    fn letters_are_distinct_and_ordered() {
        assert_eq!(FocusRule::Key('e').letters(), vec!['e']);
        assert_eq!(FocusRule::Contains("cr").letters(), vec!['c', 'r']);
        assert_eq!(
            FocusRule::Suffix("tion").letters(),
            vec!['t', 'i', 'o', 'n']
        );
        // A repeated letter lights up one tile, not two.
        assert_eq!(FocusRule::Suffix("ed").letters(), vec!['e', 'd']);
    }

    #[test]
    fn labels_are_uppercase_with_dashed_suffixes() {
        assert_eq!(FocusRule::Key('e').label(), "E");
        assert_eq!(FocusRule::Contains("cr").label(), "CR");
        assert_eq!(FocusRule::Suffix("tion").label(), "-TION");
    }

    #[test]
    fn config_values_are_lowercase_with_dashed_suffixes() {
        assert_eq!(FocusRule::Key('e').config_value(), "e");
        assert_eq!(FocusRule::Contains("cr").config_value(), "cr");
        assert_eq!(FocusRule::Suffix("tion").config_value(), "-tion");
    }

    #[test]
    fn config_value_round_trips_for_every_preset() {
        for rule in FOCUS_PATTERNS {
            let value = rule.config_value();
            assert_eq!(
                FocusRule::from_config(&value),
                Some(*rule),
                "preset {value} did not round-trip"
            );
        }
    }

    #[test]
    fn config_value_round_trips_for_every_letter() {
        for c in 'a'..='z' {
            let rule = FocusRule::Key(c);
            assert_eq!(FocusRule::from_config(&rule.config_value()), Some(rule));
        }
    }

    #[test]
    fn from_config_rejects_junk() {
        // Empty and lone separator.
        assert_eq!(FocusRule::from_config(""), None);
        assert_eq!(FocusRule::from_config("-"), None);
        // Uppercase: config_value is lowercase, so this never round-trips.
        assert_eq!(FocusRule::from_config("E"), None);
        assert_eq!(FocusRule::from_config("CR"), None);
        assert_eq!(FocusRule::from_config("-TION"), None);
        // Non-letters and whitespace.
        assert_eq!(FocusRule::from_config("1"), None);
        assert_eq!(FocusRule::from_config(" "), None);
        assert_eq!(FocusRule::from_config("cr "), None);
        // Multi-letter strings that are not presets.
        assert_eq!(FocusRule::from_config("sk"), None);
        assert_eq!(FocusRule::from_config("banana"), None);
        assert_eq!(FocusRule::from_config("-ness"), None);
        // Right spelling, wrong variant: "ing" is a suffix preset, so the
        // bare form must not resolve to a Contains rule.
        assert_eq!(FocusRule::from_config("ing"), None);
        assert_eq!(FocusRule::from_config("-cr"), None);
    }

    #[test]
    fn preset_list_is_the_documented_set() {
        assert_eq!(FOCUS_PATTERNS.len(), 16);
        // Config values are the persistence keys, so duplicates would make
        // a pin ambiguous.
        let mut values: Vec<String> = FOCUS_PATTERNS.iter().map(|r| r.config_value()).collect();
        values.sort();
        let count = values.len();
        values.dedup();
        assert_eq!(values.len(), count, "duplicate config values in presets");
        // No preset may collide with a single-letter pin.
        assert!(FOCUS_PATTERNS.iter().all(|r| r.config_value().len() > 1));
    }
}
