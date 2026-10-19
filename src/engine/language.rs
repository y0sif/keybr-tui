//! Practice languages and everything that differs between them.
//!
//! The engine itself is language-agnostic: keybr.com's algorithm only needs
//! an alphabet, a phonetic model trained on that alphabet, a word list, and
//! the order in which letters unlock. The model file carries the first and
//! the last of those (its alphabet header and its letter frequencies), and
//! the word list sits beside it, all copied from the keybr.com source tree
//! (see `data/README.md`). Adding a language is adding its two data files,
//! one variant here and one row in `SPECS`.

use std::sync::OnceLock;

use crate::engine::filter::{FocusRule, FOCUS_PATTERNS};
use crate::engine::transition::ModelLetters;

/// A practice language. English is the default and keeps every file name,
/// stat and config value exactly as they were before languages existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Turkish,
    German,
}

/// Everything about a language that is data rather than code.
struct LanguageSpec {
    /// ISO 639-1 code: the config value and keybr.com's language id.
    code: &'static str,
    /// Name shown in the UI, in the language itself.
    display_name: &'static str,
    /// Other names `from_code` accepts, lowercase.
    aliases: &'static [&'static str],
    /// Embedded keybr.com phonetic model (`model-<code>.data`).
    model: &'static [u8],
    /// Embedded word list (`wordlist-<code>.txt`), one word per line.
    wordlist: &'static str,
    /// Combination drills (see `Language::focus_patterns`).
    focus_patterns: &'static [FocusRule],
    /// Uppercase forms that differ from Rust's locale-free
    /// `to_uppercase`, as in keybr.com's `Language.letterName`.
    upper_overrides: &'static [(char, &'static str)],
    /// Lowercase forms that differ from Rust's locale-free `to_lowercase`.
    lower_overrides: &'static [(char, char)],
}

/// One row per language, in `Language::ALL` order, so `Language as usize`
/// indexes it.
static SPECS: [LanguageSpec; Language::ALL.len()] = [
    LanguageSpec {
        code: "en",
        display_name: "English",
        aliases: &["english"],
        model: include_bytes!("../../data/model-en.data"),
        wordlist: include_str!("../../data/wordlist-en.txt"),
        focus_patterns: FOCUS_PATTERNS,
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "tr",
        display_name: "Türkçe",
        aliases: &["turkish", "türkçe"],
        model: include_bytes!("../../data/model-tr.data"),
        wordlist: include_str!("../../data/wordlist-tr.txt"),
        focus_patterns: &[],
        // Dotted and dotless i are separate letters, so `i` must become
        // `İ` and `I` must become `ı`; Rust would merge them.
        upper_overrides: &[('i', "İ")],
        lower_overrides: &[('I', 'ı'), ('İ', 'i')],
    },
    LanguageSpec {
        code: "de",
        display_name: "Deutsch",
        aliases: &["german", "deutsch"],
        model: include_bytes!("../../data/model-de.data"),
        wordlist: include_str!("../../data/wordlist-de.txt"),
        focus_patterns: &[],
        // keybr.com shows the capital Eszett; Rust would give "SS".
        upper_overrides: &[('ß', "ẞ")],
        lower_overrides: &[],
    },
];

/// Letters and unlock order read out of each language's model, on first use.
static MODEL_LETTERS: [OnceLock<ModelLetters>; Language::ALL.len()] =
    [const { OnceLock::new() }; Language::ALL.len()];

// `SPECS` and `MODEL_LETTERS` are indexed by `Language as usize`.
const _: () = {
    let mut i = 0;
    while i < Language::ALL.len() {
        assert!(
            Language::ALL[i] as usize == i,
            "Language::ALL must list variants in order"
        );
        i += 1;
    }
};

impl Language {
    /// Every supported language, in the order the UI lists them.
    pub const ALL: &'static [Language] = &[Language::English, Language::Turkish, Language::German];

    fn spec(self) -> &'static LanguageSpec {
        &SPECS[self as usize]
    }

    fn model_letters(self) -> &'static ModelLetters {
        MODEL_LETTERS[self as usize].get_or_init(|| {
            ModelLetters::read(self.model_bytes())
                .unwrap_or_else(|e| panic!("embedded model-{}.data is malformed: {e}", self.code()))
        })
    }

    /// Every supported code, comma-separated (`"en, tr, de"`), for help
    /// and error text.
    pub fn supported_codes() -> String {
        Language::ALL
            .iter()
            .map(|l| l.code())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// ISO 639-1 code: the config value and keybr.com's language id.
    pub fn code(self) -> &'static str {
        self.spec().code
    }

    /// Parse a language code (`"en"`, `"tr"`) or name. Case-insensitive,
    /// since it comes from a hand-edited config file or the command line.
    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.trim().to_lowercase();
        Language::ALL.iter().copied().find(|l| {
            let spec = l.spec();
            spec.code == code || spec.aliases.contains(&code.as_str())
        })
    }

    /// Name shown in the UI, in the language itself.
    pub fn display_name(self) -> &'static str {
        self.spec().display_name
    }

    /// The letters of the alphabet in display order (the heatmap row): the
    /// model's alphabet header after its leading space. keybr.com writes
    /// that header from `Language.alphabet`, its display order.
    pub fn letters(self) -> &'static [char] {
        &self.model_letters().letters
    }

    /// True if `c` is a letter of this language's practice alphabet.
    pub fn is_letter(self, c: char) -> bool {
        self.letters().contains(&c)
    }

    /// Order in which letters unlock. The first six are the starter set.
    /// keybr.com's `Letter.frequencyOrder` over the model: letters sorted
    /// by their summed transition frequency, highest first.
    pub fn unlock_order(self) -> &'static [char] {
        &self.model_letters().frequency_order
    }

    /// Embedded keybr.com phonetic model (`model-<code>.data`).
    pub fn model_bytes(self) -> &'static [u8] {
        self.spec().model
    }

    /// Embedded word list (`wordlist-<code>.txt`), one word per line.
    pub fn wordlist(self) -> &'static str {
        self.spec().wordlist
    }

    /// Combination drills this language offers. The presets are English
    /// bigrams and endings, which keybr.com does not have at all, so other
    /// languages simply offer none.
    pub fn focus_patterns(self) -> &'static [FocusRule] {
        self.spec().focus_patterns
    }

    /// True if a keybr.com layout id (`"en-us"`, `"tr-tr-q"`) belongs to
    /// this language. keybr layout ids start with the language code, then
    /// a `-` or, for some (`"en_focal"`), a `_`.
    pub fn owns_layout(self, layout: &str) -> bool {
        layout
            .strip_prefix(self.code())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
    }

    /// Uppercase form of a letter for display. The language's own rules
    /// come first (Turkish `i` is `İ`, German `ß` is `ẞ`), then Rust's
    /// locale-free uppercase.
    pub fn upper(self, c: char) -> String {
        match self
            .spec()
            .upper_overrides
            .iter()
            .find(|&&(from, _)| from == c)
        {
            Some(&(_, upper)) => upper.to_string(),
            None => c.to_uppercase().collect(),
        }
    }

    /// Uppercase form of a whole string, with the same rules.
    pub fn upper_str(self, s: &str) -> String {
        s.chars().map(|c| self.upper(c)).collect()
    }

    /// Lowercase form of a letter typed or hand-edited in uppercase.
    pub fn lower(self, c: char) -> char {
        match self
            .spec()
            .lower_overrides
            .iter()
            .find(|&&(from, _)| from == c)
        {
            Some(&(_, lower)) => lower,
            None => c.to_lowercase().next().unwrap_or(c),
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
    use crate::engine::filter::LetterFilter;
    use crate::engine::transition::TransitionTable;
    use crate::engine::WordGenerator;
    use std::collections::{HashMap, HashSet};

    /// The alphabets and unlock orders this crate shipped as hand-copied
    /// constants before they were read from the models.
    const EN_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
    const EN_UNLOCK_ORDER: &str = "eniarltosudycghpmkbwfzvxqj";
    const TR_LETTERS: &str = "abcçdefgğhıijklmnoöprsştuüvyz";
    const TR_UNLOCK_ORDER: &str = "aeinırlutmküoysdzşcbğhfçpgvöj";
    /// keybr.com's `Language.DE` alphabet and its `Letter.frequencyOrder`
    /// over `model-de.data`.
    const DE_LETTERS: &str = "aäbcdefghijklmnoöpqrstuüvwxyzß";
    const DE_UNLOCK_ORDER: &str = "entirlhascgumofdkbpzüäßwövyqxj";

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    /// keybr.com's `Letter.frequencyOrder`, re-derived through the decoded
    /// dense table rather than the sparse pass `unlock_order` uses: sum
    /// each letter's frequency over every segment of the model, sort
    /// descending, ties by code point.
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
    fn derived_letters_and_unlock_order_match_the_old_constants() {
        let expected = [
            (Language::English, EN_LETTERS, EN_UNLOCK_ORDER),
            (Language::Turkish, TR_LETTERS, TR_UNLOCK_ORDER),
            (Language::German, DE_LETTERS, DE_UNLOCK_ORDER),
        ];
        for (lang, letters, order) in expected {
            assert_eq!(lang.letters(), chars(letters).as_slice(), "{}", lang.code());
            assert_eq!(
                lang.unlock_order(),
                chars(order).as_slice(),
                "{}",
                lang.code()
            );
        }
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
        assert_eq!(Language::from_code("Türkçe"), Some(Language::Turkish));
        assert_eq!(Language::from_code("de"), Some(Language::German));
        assert_eq!(Language::from_code(" Deutsch "), Some(Language::German));
        assert_eq!(Language::from_code("GERMAN"), Some(Language::German));
        assert_eq!(Language::from_code("xx"), None);
        assert_eq!(Language::supported_codes(), "en, tr, de");
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
    fn german_eszett_shows_as_capital_eszett() {
        let de = Language::German;
        assert_eq!(de.upper('ß'), "ẞ");
        assert_eq!(de.lower('ẞ'), 'ß');
        assert_eq!(de.upper('ä'), "Ä");
        assert_eq!(de.lower('Ü'), 'ü');
        assert_eq!(de.upper('i'), "I");
    }

    #[test]
    fn every_letter_uppercases_to_one_char() {
        for &lang in Language::ALL {
            for &c in lang.letters() {
                let upper = lang.upper(c);
                assert_eq!(
                    upper.chars().count(),
                    1,
                    "{} '{c}' uppercases to {upper:?}",
                    lang.code()
                );
                assert_eq!(
                    lang.lower(upper.chars().next().unwrap()),
                    c,
                    "{}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn every_wordlist_uses_only_its_own_letters() {
        for &lang in Language::ALL {
            let mut count = 0;
            for word in lang.wordlist().lines() {
                assert!(
                    word.chars().all(|c| lang.is_letter(c)),
                    "{} word {word:?} has a letter outside its alphabet",
                    lang.code()
                );
                let n = word.chars().count();
                assert!((2..=12).contains(&n), "{} word {word:?}", lang.code());
                count += 1;
            }
            assert!(count >= 5000, "{} has only {count} words", lang.code());
        }
    }

    #[test]
    fn german_model_and_generator_use_german_letters() {
        let de = Language::German;
        assert_eq!(de.letters().len(), 30);
        assert_eq!(&de.unlock_order()[..6], &['e', 'n', 't', 'i', 'r', 'l']);
        // A focus on an umlaut, with and without the real-word blend.
        let filter = LetterFilter::new(de.letters(), Some(FocusRule::Key('ä')));
        for natural in [true, false] {
            let mut gen = WordGenerator::with_seed_for(11, de);
            gen.set_natural_words(natural);
            let text = gen.generate_fragment(&filter, 200);
            assert!(text.chars().count() >= 200, "{text}");
            for word in text.split_whitespace() {
                assert!(word.contains('ä'), "focus 'ä' missing from '{word}'");
                assert!(word.chars().all(|c| de.is_letter(c)), "'{word}'");
            }
        }
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
        assert_eq!(Language::German.stats_file_name(), "stats-de.json");
    }
}
