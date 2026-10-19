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
use crate::engine::layout;
use crate::engine::transition::ModelLetters;

/// A practice language. English is the default and keeps every file name,
/// stat and config value exactly as they were before languages existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    EnglishUk,
    Belarusian,
    Czech,
    Danish,
    German,
    Greek,
    Spanish,
    Estonian,
    Finnish,
    French,
    Croatian,
    Hungarian,
    Armenian,
    Italian,
    Lithuanian,
    Latvian,
    NorwegianBokmal,
    Dutch,
    Polish,
    Portuguese,
    Romanian,
    Russian,
    Slovenian,
    Swedish,
    Turkish,
    Ukrainian,
}

/// Everything about a language that is data rather than code.
struct LanguageSpec {
    /// The config value: keybr.com's language id, lowercase (`en-gb` is
    /// its `en-GB`).
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
    /// `to_uppercase`, as in keybr.com's `Language.letterName` (Greek
    /// aside, see its row).
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
        code: "en-gb",
        display_name: "English (UK)",
        aliases: &["english (uk)", "en_gb", "british"],
        model: include_bytes!("../../data/model-en-gb.data"),
        wordlist: include_str!("../../data/wordlist-en-gb.txt"),
        // English bigrams and endings, so the English drills apply.
        focus_patterns: FOCUS_PATTERNS,
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "be",
        display_name: "Беларуская",
        aliases: &["belarusian", "беларуская"],
        model: include_bytes!("../../data/model-be.data"),
        wordlist: include_str!("../../data/wordlist-be.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "cs",
        display_name: "Čeština",
        aliases: &["czech", "čeština"],
        model: include_bytes!("../../data/model-cs.data"),
        wordlist: include_str!("../../data/wordlist-cs.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "da",
        display_name: "Dansk",
        aliases: &["danish", "dansk"],
        model: include_bytes!("../../data/model-da.data"),
        wordlist: include_str!("../../data/wordlist-da.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
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
    LanguageSpec {
        code: "el",
        display_name: "Ελληνικά",
        aliases: &["greek", "ελληνικά"],
        model: include_bytes!("../../data/model-el.data"),
        wordlist: include_str!("../../data/wordlist-el.txt"),
        focus_patterns: &[],
        // Deliberately not keybr.com's tiles. Its Greek uppercase drops
        // the tonos (`ά` shows as `Α`, the tile of `α`) and gives `ς` the
        // `Σ` of `σ`, so seven pairs of tiles look alike there. Rust keeps
        // the accented capitals (`Ά Έ Ή Ί Ό Ύ Ώ`), and final sigma shows
        // as itself.
        upper_overrides: &[('ς', "ς")],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "es",
        display_name: "Español",
        aliases: &["spanish", "español"],
        model: include_bytes!("../../data/model-es.data"),
        wordlist: include_str!("../../data/wordlist-es.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "et",
        display_name: "Eesti",
        aliases: &["estonian", "eesti"],
        model: include_bytes!("../../data/model-et.data"),
        wordlist: include_str!("../../data/wordlist-et.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "fi",
        display_name: "Suomi",
        aliases: &["finnish", "suomi"],
        model: include_bytes!("../../data/model-fi.data"),
        wordlist: include_str!("../../data/wordlist-fi.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "fr",
        display_name: "Français",
        aliases: &["french", "français"],
        model: include_bytes!("../../data/model-fr.data"),
        wordlist: include_str!("../../data/wordlist-fr.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "hr",
        display_name: "Hrvatski",
        aliases: &["croatian", "hrvatski"],
        model: include_bytes!("../../data/model-hr.data"),
        wordlist: include_str!("../../data/wordlist-hr.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "hu",
        display_name: "Magyar",
        aliases: &["hungarian", "magyar"],
        model: include_bytes!("../../data/model-hu.data"),
        wordlist: include_str!("../../data/wordlist-hu.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "hy",
        display_name: "Հայերեն",
        aliases: &["armenian", "հայերեն"],
        model: include_bytes!("../../data/model-hy.data"),
        wordlist: include_str!("../../data/wordlist-hy.txt"),
        focus_patterns: &[],
        // The ligature `և` has no capital letter. keybr.com shows it as
        // `ԵՎ`, as reformed spelling capitalises it; Rust would give
        // `ԵՒ`. Its tile is the one two-letter label. No single capital
        // lowercases back to it, so a pin on it is written as `և`.
        upper_overrides: &[('և', "ԵՎ")],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "it",
        display_name: "Italiano",
        aliases: &["italian", "italiano"],
        model: include_bytes!("../../data/model-it.data"),
        wordlist: include_str!("../../data/wordlist-it.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "lt",
        display_name: "Lietuvių",
        aliases: &["lithuanian", "lietuvių"],
        model: include_bytes!("../../data/model-lt.data"),
        wordlist: include_str!("../../data/wordlist-lt.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "lv",
        display_name: "Latviešu",
        aliases: &["latvian", "latviešu"],
        model: include_bytes!("../../data/model-lv.data"),
        wordlist: include_str!("../../data/wordlist-lv.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "nb",
        display_name: "Norsk bokmål",
        aliases: &[
            "norwegian bokmål",
            "norsk bokmål",
            "norwegian",
            "bokmål",
            "bokmal",
        ],
        model: include_bytes!("../../data/model-nb.data"),
        wordlist: include_str!("../../data/wordlist-nb.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "nl",
        display_name: "Nederlands",
        aliases: &["dutch", "nederlands"],
        model: include_bytes!("../../data/model-nl.data"),
        wordlist: include_str!("../../data/wordlist-nl.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "pl",
        display_name: "Polski",
        aliases: &["polish", "polski"],
        model: include_bytes!("../../data/model-pl.data"),
        wordlist: include_str!("../../data/wordlist-pl.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "pt",
        display_name: "Português",
        aliases: &["portuguese", "português"],
        model: include_bytes!("../../data/model-pt.data"),
        wordlist: include_str!("../../data/wordlist-pt.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "ro",
        display_name: "Română",
        aliases: &["romanian", "română"],
        model: include_bytes!("../../data/model-ro.data"),
        wordlist: include_str!("../../data/wordlist-ro.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "ru",
        display_name: "Русский",
        aliases: &["russian", "русский"],
        model: include_bytes!("../../data/model-ru.data"),
        wordlist: include_str!("../../data/wordlist-ru.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "sl",
        display_name: "Slovenščina",
        aliases: &["slovenian", "slovenščina"],
        model: include_bytes!("../../data/model-sl.data"),
        wordlist: include_str!("../../data/wordlist-sl.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
        lower_overrides: &[],
    },
    LanguageSpec {
        code: "sv",
        display_name: "Svenska",
        aliases: &["swedish", "svenska"],
        model: include_bytes!("../../data/model-sv.data"),
        wordlist: include_str!("../../data/wordlist-sv.txt"),
        focus_patterns: &[],
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
        code: "uk",
        display_name: "Українська",
        aliases: &["ukrainian", "українська"],
        model: include_bytes!("../../data/model-uk.data"),
        wordlist: include_str!("../../data/wordlist-uk.txt"),
        focus_patterns: &[],
        upper_overrides: &[],
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
    /// Every supported language, in the order the UI lists them: both
    /// Englishes, then the rest by code.
    pub const ALL: &'static [Language] = &[
        Language::English,
        Language::EnglishUk,
        Language::Belarusian,
        Language::Czech,
        Language::Danish,
        Language::German,
        Language::Greek,
        Language::Spanish,
        Language::Estonian,
        Language::Finnish,
        Language::French,
        Language::Croatian,
        Language::Hungarian,
        Language::Armenian,
        Language::Italian,
        Language::Lithuanian,
        Language::Latvian,
        Language::NorwegianBokmal,
        Language::Dutch,
        Language::Polish,
        Language::Portuguese,
        Language::Romanian,
        Language::Russian,
        Language::Slovenian,
        Language::Swedish,
        Language::Turkish,
        Language::Ukrainian,
    ];

    fn spec(self) -> &'static LanguageSpec {
        &SPECS[self as usize]
    }

    fn model_letters(self) -> &'static ModelLetters {
        MODEL_LETTERS[self as usize].get_or_init(|| {
            ModelLetters::read(self.model_bytes())
                .unwrap_or_else(|e| panic!("embedded model-{}.data is malformed: {e}", self.code()))
        })
    }

    /// Every supported code, comma-separated in `ALL` order
    /// (`"en, en-gb, be, ..."`), for help and error text.
    pub fn supported_codes() -> String {
        Language::ALL
            .iter()
            .map(|l| l.code())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The config value: keybr.com's language id, lowercase (`en-gb` is
    /// its `en-GB`).
    pub fn code(self) -> &'static str {
        self.spec().code
    }

    /// Parse a language code (`"en"`, `"en-gb"`) or name. Case-insensitive,
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
    /// this language: the language keybr.com's layout table gives it. An
    /// id the table lacks (one keybr.com added later) goes by the shape
    /// every id in the table has: the language code, then the end, a `-`
    /// or, for some (`"en_focal"`), a `_`.
    pub fn owns_layout(self, id: &str) -> bool {
        match layout::lookup(id) {
            Some(info) => info.language == self.code(),
            None => id
                .strip_prefix(self.code())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_'])),
        }
    }

    /// Uppercase form of a letter for display. The language's own rules
    /// come first (Turkish `i` is `İ`, German `ß` is `ẞ`), then Rust's
    /// locale-free uppercase. One character, except Armenian `և`, whose
    /// form is the two letters `ԵՎ`.
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

    /// Lowercase form of a letter typed or hand-edited in uppercase. A pin
    /// is one character, so Armenian `և` (shown as `ԵՎ`) is pinned as `և`
    /// itself, and a lone `Ե` is the letter `ե`.
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
    use ratatui::text::Span;
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
    /// keybr.com's alphabets for the rest, and the unlock orders derived
    /// from their models outside this crate the same way.
    const EN_GB_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
    const EN_GB_UNLOCK_ORDER: &str = "eitnarsloudychmgpkbfvwzxqj";
    const BE_LETTERS: &str = "абвгдежзійклмнопрстуўфхцчшыьэюя";
    const BE_UNLOCK_ORDER: &str = "аыінеуолцкрятмўсьвдчзюйгхэшбпжф";
    const CS_LETTERS: &str = "aábcčdďeéěfghiíjklmnňoóprřsštťuúůvxyýzž";
    const CS_UNLOCK_ORDER: &str = "ealiotnukímvhdcěrysájzéýžčřšbpůťňfgďxúó";
    const DA_LETTERS: &str = "abcdefghijklmnopqrstuvwxyzæøå";
    const DA_UNLOCK_ORDER: &str = "ertnilasdgkomuævpbøyfjåchzwxq";
    const EL_LETTERS: &str = "αάβγδεέζηήθιίκλμνξοόπρσςτυύφχψωώ";
    const EL_UNLOCK_ORDER: &str = "εαοιτνσρςημλίάκυόωέήπγύθδχώφζβξψ";
    const ES_LETTERS: &str = "aábcdeéfghiíjlmnñoópqrstuúvxyz";
    const ES_UNLOCK_ORDER: &str = "aeoirnstulcdmgbópívájhéfzqyñúx";
    const ET_LETTERS: &str = "abdefghijklmnoprstuvõäöüš";
    const ET_UNLOCK_ORDER: &str = "aeisumtlnokrdgäpvhöübjõfš";
    const FI_LETTERS: &str = "abdefghijklmnoprstuvyäö";
    const FI_UNLOCK_ORDER: &str = "aintsäelkuomyrpjvdhögfb";
    const FR_LETTERS: &str = "abcçdeéèfghijlmnopqrstuvxyz";
    const FR_UNLOCK_ORDER: &str = "eirnutsaloémcdgphèbfvqxzyjç";
    const HR_LETTERS: &str = "abcćčdđefghijklmnoprsštuvzž";
    const HR_UNLOCK_ORDER: &str = "aietonrjlkusdvmczćčgbpšžhfđ";
    const HU_LETTERS: &str = "aábcdeéfghiíjklmnoóöőpqrstuúüűvwxyz";
    const HU_UNLOCK_ORDER: &str = "eatnlskiroázégbymdóuvcöpőjhíüúfűxwq";
    const HY_LETTERS: &str = "աբգդեզէըթժիլխծկհձղճմյնշոչպջռսվտրցւփքևօֆ";
    const HY_UNLOCK_ORDER: &str = "աոնեիրւլտկվյմսցըդքղթծգբհպռխզչշջևճփժձֆէօ";
    const IT_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
    const IT_UNLOCK_ORDER: &str = "ieoatrnlcsumgzdbpvfhqkwyxj";
    const LT_LETTERS: &str = "aąbcčdeęėfghiįyjklmnoprsštuųūvzž";
    const LT_UNLOCK_ORDER: &str = "iasoetunkrlėjmdąųgyvbšpįžčęūczhf";
    const LV_LETTERS: &str = "aābcčdeēfgģhiījkķlļmnņoprsštuūvzž";
    const LV_UNLOCK_ORDER: &str = "iasternlmouādkēījgzbcpvšņūķļģfžhč";
    const NB_LETTERS: &str = "abcdefghijklmnoprstuvwyæøå";
    const NB_UNLOCK_ORDER: &str = "ertnilaskodgmpuvøyjfbåæhwc";
    const NL_LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
    const NL_UNLOCK_ORDER: &str = "eaintorlsudkgmhcjpfbwvzyqx";
    const PL_LETTERS: &str = "aąbcćdeęfghijklłmnńoóprsśtuwyzźż";
    const PL_UNLOCK_ORDER: &str = "ieaoznywrctkudmjęslłćąhbóśgżpfńź";
    const PT_LETTERS: &str = "aáâãàbcçdeéêfghiíjlmnoóôõpqrstuúvxz";
    const PT_UNLOCK_ORDER: &str = "aoiersntudclmgãvhbpçfízéxõáqóêjâúôà";
    const RO_LETTERS: &str = "aăâbcdefghiîjlmnoprsștțuvxz";
    const RO_UNLOCK_ORDER: &str = "ieatrnulăocsmdpzbgvâțfhjșxî";
    const RU_LETTERS: &str = "абвгдежзийклмнопрстуфхцчшщъыьэюя";
    const RU_UNLOCK_ORDER: &str = "атиоенркслйвьыдумячзжгбпцшщхюфэъ";
    const SL_LETTERS: &str = "abcčdefghijklmnoprsštuvzž";
    const SL_UNLOCK_ORDER: &str = "aeitonrkjslvdčmbucgpzšžhf";
    const SV_LETTERS: &str = "abcdefghijklmnoprstuvwxyåäö";
    const SV_UNLOCK_ORDER: &str = "atrenilsdgkomäupöåvcfyjhbxw";
    const UK_LETTERS: &str = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюя";
    const UK_UNLOCK_ORDER: &str = "иаоінвтуелркмяьдсчюгйхзцпшбжїєґщф";

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
    fn derived_letters_and_unlock_order_match_the_pinned_constants() {
        let expected = [
            (Language::English, EN_LETTERS, EN_UNLOCK_ORDER),
            (Language::EnglishUk, EN_GB_LETTERS, EN_GB_UNLOCK_ORDER),
            (Language::Belarusian, BE_LETTERS, BE_UNLOCK_ORDER),
            (Language::Czech, CS_LETTERS, CS_UNLOCK_ORDER),
            (Language::Danish, DA_LETTERS, DA_UNLOCK_ORDER),
            (Language::German, DE_LETTERS, DE_UNLOCK_ORDER),
            (Language::Greek, EL_LETTERS, EL_UNLOCK_ORDER),
            (Language::Spanish, ES_LETTERS, ES_UNLOCK_ORDER),
            (Language::Estonian, ET_LETTERS, ET_UNLOCK_ORDER),
            (Language::Finnish, FI_LETTERS, FI_UNLOCK_ORDER),
            (Language::French, FR_LETTERS, FR_UNLOCK_ORDER),
            (Language::Croatian, HR_LETTERS, HR_UNLOCK_ORDER),
            (Language::Hungarian, HU_LETTERS, HU_UNLOCK_ORDER),
            (Language::Armenian, HY_LETTERS, HY_UNLOCK_ORDER),
            (Language::Italian, IT_LETTERS, IT_UNLOCK_ORDER),
            (Language::Lithuanian, LT_LETTERS, LT_UNLOCK_ORDER),
            (Language::Latvian, LV_LETTERS, LV_UNLOCK_ORDER),
            (Language::NorwegianBokmal, NB_LETTERS, NB_UNLOCK_ORDER),
            (Language::Dutch, NL_LETTERS, NL_UNLOCK_ORDER),
            (Language::Polish, PL_LETTERS, PL_UNLOCK_ORDER),
            (Language::Portuguese, PT_LETTERS, PT_UNLOCK_ORDER),
            (Language::Romanian, RO_LETTERS, RO_UNLOCK_ORDER),
            (Language::Russian, RU_LETTERS, RU_UNLOCK_ORDER),
            (Language::Slovenian, SL_LETTERS, SL_UNLOCK_ORDER),
            (Language::Swedish, SV_LETTERS, SV_UNLOCK_ORDER),
            (Language::Turkish, TR_LETTERS, TR_UNLOCK_ORDER),
            (Language::Ukrainian, UK_LETTERS, UK_UNLOCK_ORDER),
        ];
        let pinned: Vec<Language> = expected.iter().map(|&(lang, _, _)| lang).collect();
        assert_eq!(pinned, Language::ALL, "every language pins its letters");
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
        assert_eq!(Language::from_code("EN-GB"), Some(Language::EnglishUk));
        assert_eq!(Language::from_code("en_GB"), Some(Language::EnglishUk));
        assert_eq!(Language::from_code("British"), Some(Language::EnglishUk));
        assert_eq!(Language::from_code("ΕΛΛΗΝΙΚΆ"), Some(Language::Greek));
        assert_eq!(Language::from_code("Русский"), Some(Language::Russian));
        assert_eq!(
            Language::from_code("bokmal"),
            Some(Language::NorwegianBokmal)
        );
        assert_eq!(
            Language::from_code("Norwegian"),
            Some(Language::NorwegianBokmal)
        );
        assert_eq!(Language::from_code("uk"), Some(Language::Ukrainian));
        assert_eq!(Language::from_code("xx"), None);
        assert_eq!(
            Language::supported_codes(),
            "en, en-gb, be, cs, da, de, el, es, et, fi, fr, hr, hu, hy, it, lt, lv, nb, nl, pl, pt, \
             ro, ru, sl, sv, tr, uk"
        );
    }

    /// `from_code` takes the first match, so a name two languages shared
    /// would quietly pick one of them.
    #[test]
    fn every_code_and_alias_names_one_language() {
        let mut owners: HashMap<&str, Language> = HashMap::new();
        for &lang in Language::ALL {
            let spec = lang.spec();
            for &name in std::iter::once(&spec.code).chain(spec.aliases) {
                assert_eq!(name, name.trim().to_lowercase(), "{name:?}");
                if let Some(other) = owners.insert(name, lang) {
                    panic!("{name:?} names {other:?} and {lang:?}");
                }
            }
            // The native name is always accepted.
            assert_eq!(Language::from_code(lang.display_name()), Some(lang));
        }
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
    fn greek_keeps_its_accented_capitals_apart() {
        let el = Language::Greek;
        assert_eq!(el.upper('α'), "Α");
        assert_eq!(el.upper('ά'), "Ά");
        assert_eq!(el.upper('ή'), "Ή");
        assert_eq!(el.upper('ώ'), "Ώ");
        assert_eq!(el.upper('σ'), "Σ");
        assert_eq!(el.upper('ς'), "ς");
        assert_eq!(el.lower('Σ'), 'σ');
        assert_eq!(el.lower('ς'), 'ς');
        assert_eq!(el.lower('Ύ'), 'ύ');
    }

    #[test]
    fn armenian_ech_yiwn_shows_as_two_capitals() {
        let hy = Language::Armenian;
        assert_eq!(hy.upper('և'), "ԵՎ");
        assert_eq!(hy.upper('ե'), "Ե");
        assert_eq!(hy.upper('վ'), "Վ");
        assert_eq!(hy.upper_str("ևս"), "ԵՎՍ");
        // A pin stays one character: `և` itself, and `Ե` is `ե`.
        assert_eq!(hy.lower('և'), 'և');
        assert_eq!(hy.lower('Ե'), 'ե');
    }

    /// Every heatmap tile has a label of its own: one character a cell
    /// wide (two for Armenian `և`, `ԵՎ`), and no two letters of a language
    /// share one. That last part is why Greek keeps Rust's accented
    /// capitals rather than keybr.com's. A single-character label typed
    /// back as a pin lowercases to its letter.
    #[test]
    fn every_letter_has_a_tile_label_of_its_own() {
        for &lang in Language::ALL {
            let mut shown: HashMap<String, char> = HashMap::new();
            for &c in lang.letters() {
                let upper = lang.upper(c);
                let n = upper.chars().count();
                let expected = if (lang, c) == (Language::Armenian, 'և') {
                    2
                } else {
                    1
                };
                assert_eq!(n, expected, "{} '{c}' shows as {upper:?}", lang.code());
                assert_eq!(
                    Span::raw(upper.clone()).width(),
                    n,
                    "{} '{c}' shows as {upper:?}",
                    lang.code()
                );
                if let Some(other) = shown.insert(upper.clone(), c) {
                    panic!("{} '{other}' and '{c}' both show as {upper:?}", lang.code());
                }
                if n == 1 {
                    assert_eq!(
                        lang.lower(upper.chars().next().unwrap()),
                        c,
                        "{}",
                        lang.code()
                    );
                }
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
            // keybr.com's lists hold 5,000 to 10,000 words, except
            // Slovenian's, which has 2,765.
            let floor = if lang == Language::Slovenian {
                2500
            } else {
                5000
            };
            assert!(count >= floor, "{} has only {count} words", lang.code());
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

    /// Every language makes a lesson out of its six starter letters and
    /// out of its whole alphabet: focused on the rarest letter in play,
    /// with and without the real-word blend, and phonetic with no focus.
    #[test]
    fn every_language_generates_lessons_from_its_own_letters() {
        for &lang in Language::ALL {
            let mut gen = WordGenerator::with_seed_for(5, lang);
            let starters = &lang.unlock_order()[..6];
            for letters in [starters, lang.unlock_order()] {
                let rarest = Some(FocusRule::Key(*letters.last().unwrap()));
                for (focus, natural) in [(rarest, true), (rarest, false), (None, false)] {
                    gen.set_natural_words(natural);
                    let text = gen.generate_fragment(&LetterFilter::new(letters, focus), 60);
                    let n = letters.len();
                    assert!(
                        text.chars().count() >= 60,
                        "{} with {n} letters: {text:?}",
                        lang.code()
                    );
                    assert!(
                        text.chars().all(|c| c == ' ' || letters.contains(&c)),
                        "{} with {n} letters: {text:?}",
                        lang.code()
                    );
                }
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
    fn layouts_go_to_the_language_keybr_gives_them() {
        let en = Language::English;
        assert!(en.owns_layout("en-uk"));
        assert!(!en.owns_layout("uk-ua"));
        assert!(!en.owns_layout("br-chwerty-maths"));
        assert!(!en.owns_layout("pt-br"));
        assert!(Language::German.owns_layout("de-ch"));
        assert!(Language::German.owns_layout("de-adnw-but-xcv"));
        assert!(!Language::German.owns_layout("fr-ch"));
        // Ids the table lacks fall back to the prefix shape.
        assert!(en.owns_layout("en-some-new-layout"));
        assert!(Language::German.owns_layout("de"));
        assert!(!en.owns_layout("custom"));
        assert!(!en.owns_layout("english"));
        // The table agrees with the prefix rule keybr-tui used before it
        // on every keybr.com layout, so no import changes language.
        let prefix_rule = |lang: Language, id: &str| {
            id.strip_prefix(lang.code())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
        };
        for info in layout::LAYOUTS {
            for &lang in Language::ALL {
                assert_eq!(
                    lang.owns_layout(info.id),
                    prefix_rule(lang, info.id),
                    "{} {}",
                    lang.code(),
                    info.id
                );
            }
        }
        let english = layout::LAYOUTS.iter().filter(|l| en.owns_layout(l.id));
        assert_eq!(english.count(), 37);
    }

    #[test]
    fn english_keeps_its_original_stats_file() {
        assert_eq!(Language::English.stats_file_name(), "stats.json");
        assert_eq!(Language::Turkish.stats_file_name(), "stats-tr.json");
        assert_eq!(Language::German.stats_file_name(), "stats-de.json");
        assert_eq!(Language::EnglishUk.stats_file_name(), "stats-en-gb.json");
        assert_eq!(Language::Armenian.stats_file_name(), "stats-hy.json");
    }
}
