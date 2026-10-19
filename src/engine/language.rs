//! Practice languages and everything that differs between them.
//!
//! The engine itself is language-agnostic: keybr.com's algorithm only needs
//! an alphabet, a phonetic model trained on that alphabet, a word list, and
//! the order in which letters unlock. Each of those comes from the keybr.com
//! source tree for the language (see `data/README.md`), so adding a language
//! is adding its data files and one variant here.

use crate::engine::filter::{FocusRule, FOCUS_PATTERNS};

/// A practice language. English is the default and keeps every file name,
/// stat and config value exactly as they were before languages existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Turkish,
}

/// English letters, `a-z`, in display (alphabetical) order.
const EN_LETTERS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// Turkish letters in alphabetical order, as in keybr.com's `Language.TR`
/// and the alphabet header of `model-tr.data`.
const TR_LETTERS: &[char] = &[
    'a', 'b', 'c', 'ç', 'd', 'e', 'f', 'g', 'ğ', 'h', 'ı', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'ö',
    'p', 'r', 's', 'ş', 't', 'u', 'ü', 'v', 'y', 'z',
];

/// English unlock order: keybr.com's `Letter.frequencyOrder` over
/// `model-en.data`, i.e. letters sorted by their summed transition
/// frequency, highest first.
const EN_UNLOCK_ORDER: &[char] = &[
    'e', 'n', 'i', 'a', 'r', 'l', // starter set (6)
    't', 'o', 's', 'u', 'd', 'y', 'c', 'g', 'h', 'p', 'm', 'k', 'b', 'w', 'f', 'z', 'v', 'x', 'q',
    'j',
];

/// Turkish unlock order, derived from `model-tr.data` the same way
/// (`unlock_order_matches_model_frequencies` re-derives it in tests).
const TR_UNLOCK_ORDER: &[char] = &[
    'a', 'e', 'i', 'n', 'ı', 'r', // starter set (6)
    'l', 'u', 't', 'm', 'k', 'ü', 'o', 'y', 's', 'd', 'z', 'ş', 'c', 'b', 'ğ', 'h', 'f', 'ç', 'p',
    'g', 'v', 'ö', 'j',
];

const EN_MODEL: &[u8] = include_bytes!("../../data/model-en.data");
const TR_MODEL: &[u8] = include_bytes!("../../data/model-tr.data");

const EN_WORDLIST: &str = include_str!("../../data/wordlist-en.txt");
const TR_WORDLIST: &str = include_str!("../../data/wordlist-tr.txt");

impl Language {
    /// Every supported language, in the order the UI lists them.
    #[allow(dead_code)]
    pub const ALL: &'static [Language] = &[Language::English, Language::Turkish];

    /// ISO 639-1 code: the config value and keybr.com's language id.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Turkish => "tr",
        }
    }

    /// Parse a language code (`"en"`, `"tr"`). Case-insensitive, since it
    /// comes from a hand-edited config file or the command line.
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Language::English),
            "tr" | "turkish" | "türkçe" => Some(Language::Turkish),
            _ => None,
        }
    }

    /// Name shown in the UI, in the language itself.
    pub fn display_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Turkish => "Türkçe",
        }
    }

    /// The letters of the alphabet in display order (the heatmap row).
    pub fn letters(self) -> &'static [char] {
        match self {
            Language::English => EN_LETTERS,
            Language::Turkish => TR_LETTERS,
        }
    }

    /// True if `c` is a letter of this language's practice alphabet.
    pub fn is_letter(self, c: char) -> bool {
        self.letters().contains(&c)
    }

    /// Order in which letters unlock. The first six are the starter set.
    pub fn unlock_order(self) -> &'static [char] {
        self.unlock_order_const()
    }

    /// `unlock_order`, usable in a `const` item.
    pub const fn unlock_order_const(self) -> &'static [char] {
        match self {
            Language::English => EN_UNLOCK_ORDER,
            Language::Turkish => TR_UNLOCK_ORDER,
        }
    }

    /// Embedded keybr.com phonetic model (`model-<code>.data`).
    pub fn model_bytes(self) -> &'static [u8] {
        match self {
            Language::English => EN_MODEL,
            Language::Turkish => TR_MODEL,
        }
    }

    /// Embedded word list (`wordlist-<code>.txt`), one word per line.
    pub fn wordlist(self) -> &'static str {
        match self {
            Language::English => EN_WORDLIST,
            Language::Turkish => TR_WORDLIST,
        }
    }

    /// Combination drills this language offers. The presets are English
    /// bigrams and endings, which keybr.com does not have at all, so other
    /// languages simply offer none.
    pub fn focus_patterns(self) -> &'static [FocusRule] {
        match self {
            Language::English => FOCUS_PATTERNS,
            Language::Turkish => &[],
        }
    }

    /// True if a keybr.com layout id (`"en-us"`, `"tr-tr-q"`) belongs to
    /// this language. keybr layout ids start with the language code, then
    /// a `-` or, for some (`"en_focal"`), a `_`.
    pub fn owns_layout(self, layout: &str) -> bool {
        layout
            .strip_prefix(self.code())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
    }

    /// Uppercase form of a letter for display. Turkish has dotted and
    /// dotless i as separate letters, so `i` must become `İ` there and `ı`
    /// becomes `I`; Rust's locale-free uppercase would show both as `I`.
    pub fn upper(self, c: char) -> String {
        match (self, c) {
            (Language::Turkish, 'i') => "İ".to_string(),
            _ => c.to_uppercase().collect(),
        }
    }

    /// Uppercase form of a whole string, with the same Turkish rule.
    pub fn upper_str(self, s: &str) -> String {
        s.chars().map(|c| self.upper(c)).collect()
    }

    /// Lowercase form of a letter typed or hand-edited in uppercase.
    pub fn lower(self, c: char) -> char {
        match (self, c) {
            (Language::Turkish, 'I') => 'ı',
            (Language::Turkish, 'İ') => 'i',
            _ => c.to_lowercase().next().unwrap_or(c),
        }
    }

    /// File name of this language's stats file. English keeps the original
    /// `stats.json` so existing users see no change.
    pub fn stats_file_name(self) -> String {
        match self {
            Language::English => "stats.json".to_string(),
            other => format!("stats-{}.json", other.code()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::transition::TransitionTable;
    use std::collections::{HashMap, HashSet};

    /// keybr.com's `Letter.frequencyOrder`: sum each letter's frequency
    /// over every segment of the model, sort descending, ties by code point.
    fn frequency_order(lang: Language) -> Vec<char> {
        let table = TransitionTable::for_language(lang);
        let mut totals: HashMap<char, u64> = HashMap::new();
        for (c, f) in table.letter_totals() {
            *totals.entry(c).or_default() += f;
        }
        let mut letters: Vec<char> = lang.letters().to_vec();
        letters.sort_by(|a, b| {
            totals[b]
                .cmp(&totals[a])
                .then((*a as u32).cmp(&(*b as u32)))
        });
        letters
    }

    #[test]
    fn unlock_order_matches_model_frequencies() {
        for &lang in Language::ALL {
            assert_eq!(
                lang.unlock_order(),
                frequency_order(lang).as_slice(),
                "{} unlock order drifted from its model",
                lang.code()
            );
        }
    }

    #[test]
    fn unlock_order_is_a_permutation_of_the_alphabet() {
        for &lang in Language::ALL {
            let a: HashSet<char> = lang.letters().iter().copied().collect();
            let b: HashSet<char> = lang.unlock_order().iter().copied().collect();
            assert_eq!(a, b, "{}", lang.code());
            assert_eq!(lang.letters().len(), lang.unlock_order().len());
        }
    }

    #[test]
    fn codes_round_trip() {
        for &lang in Language::ALL {
            assert_eq!(Language::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Language::from_code("TR"), Some(Language::Turkish));
        assert_eq!(Language::from_code("de"), None);
    }

    #[test]
    fn turkish_case_mapping_keeps_dotted_and_dotless_i_apart() {
        let tr = Language::Turkish;
        assert_eq!(tr.upper('i'), "İ");
        assert_eq!(tr.upper('ı'), "I");
        assert_eq!(tr.upper('ş'), "Ş");
        assert_eq!(tr.lower('I'), 'ı');
        assert_eq!(tr.lower('İ'), 'i');
        assert_eq!(Language::English.upper('i'), "I");
        assert_eq!(Language::English.lower('I'), 'i');
    }

    #[test]
    fn layouts_are_matched_by_language_prefix() {
        assert!(Language::Turkish.owns_layout("tr-tr-q"));
        assert!(Language::Turkish.owns_layout("tr-tr-f"));
        assert!(!Language::Turkish.owns_layout("en-us"));
        assert!(Language::English.owns_layout("en-us"));
        assert!(Language::English.owns_layout("en-dvorak"));
        assert!(!Language::English.owns_layout("tr-tr-q"));
        // keybr.com's Focal layout id uses an underscore.
        assert!(Language::English.owns_layout("en_focal"));
        assert!(!Language::Turkish.owns_layout("en_focal"));
        assert!(!Language::English.owns_layout("es-es"));
        assert!(!Language::Turkish.owns_layout("es-es"));
    }

    #[test]
    fn english_keeps_its_original_stats_file() {
        assert_eq!(Language::English.stats_file_name(), "stats.json");
        assert_eq!(Language::Turkish.stats_file_name(), "stats-tr.json");
    }
}
