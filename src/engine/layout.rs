//! keybr.com's keyboard layouts: the language each one types and the
//! family it belongs to.
//!
//! A keybr.com export names each session's layout by id (`"en-us"`,
//! `"tr-tr-q"`) and nothing more. keybr.com practices with the sessions of
//! the current layout's *family* (`Lesson.filter` in packages/keybr-lesson,
//! `ResultGroups.byLayoutFamily` in packages/keybr-result): {US}, {UK} and
//! Turkish Q all count as `qwerty`, Colemak keeps its own. The id does not
//! spell out its family, and a family spans languages, so this table gives
//! both the family and the language of every id.
//!
//! `LAYOUTS` is keybr.com's `Layout.ALL` (packages/keybr-keyboard/lib/layout.ts
//! at commit 05a37bc), in the order layout.ts declares the layouts, which
//! is not `Layout.ALL`'s own order. It leaves out `Layout.custom` and the
//! layouts commented out of `Layout.ALL`, which keybr.com does not offer.
//! To regenerate it, go through the `new Layout(...)` declarations in
//! layout.ts from top to bottom, skip those, and write one `row` of each
//! one's `id=`, its `language=` constant's `id=`
//! (packages/keybr-keyboard/lib/language.ts) and its `family=`.

/// One keybr.com layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutInfo {
    /// keybr.com's layout id, as an export's `layout` field carries it.
    pub id: &'static str,
    /// keybr.com's id of the language the layout types (`"en"`, `"tr"`).
    pub language: &'static str,
    /// Layouts of one family share key stats on keybr.com. Not always
    /// lowercase ASCII (`"MTGAP"`, `"йцукен"`).
    pub family: &'static str,
}

const fn row(id: &'static str, language: &'static str, family: &'static str) -> LayoutInfo {
    LayoutInfo {
        id,
        language,
        family,
    }
}

/// Every layout keybr.com offers, in layout.ts declaration order.
#[rustfmt::skip]
pub static LAYOUTS: &[LayoutInfo] = &[
    row("en-us",                             "en", "qwerty"),
    row("en-dvorak",                         "en", "dvorak"),
    row("en-dvorak-prog",                    "en", "dvorak"),
    row("en-colemak",                        "en", "colemak"),
    row("en-colemak-dh",                     "en", "colemak-dh"),
    row("en-colemak-dh-wide",                "en", "colemak-dh-wide"),
    row("en-colemak-dh-iso",                 "en", "colemak-dh-iso"),
    row("en-colemak-dh-iso-wide",            "en", "colemak-dh-iso-wide"),
    row("en-colemak-dh-matrix",              "en", "colemak-dh-matrix"),
    row("en-workman",                        "en", "workman"),
    row("en-uk",                             "en", "qwerty"),
    row("en-jp",                             "en", "qwerty"),
    row("de-de",                             "de", "qwertz"),
    row("de-ch",                             "de", "qwertz"),
    row("de-neo-2",                          "de", "de-neo-2"),
    row("de-bone",                           "de", "de-bone"),
    row("de-mine",                           "de", "de-mine"),
    row("de-noted",                          "de", "de-noted"),
    row("de-cmos",                           "de", "de-cmos"),
    row("fi-fi",                             "fi", "qwerty"),
    row("fr-fr",                             "fr", "azerty"),
    row("fr-ca",                             "fr", "qwerty"),
    row("fr-ch",                             "fr", "qwertz"),
    row("fr-bepo",                           "fr", "fr-bepo"),
    row("fr-ergol",                          "fr", "fr-ergol"),
    row("fr-optimot-ergo",                   "fr", "fr-optimot-ergo"),
    row("fr-erglace",                        "fr", "fr-erglace"),
    row("fr-ergopti",                        "fr", "fr-ergopti"),
    row("it-it",                             "it", "qwerty"),
    row("lt-lt",                             "lt", "lt-lt"),
    row("lv-lv",                             "lv", "lv-lv"),
    row("es-es",                             "es", "qwerty"),
    row("pl-pl",                             "pl", "qwerty"),
    row("pt-br",                             "pt", "qwerty"),
    row("pt-pt",                             "pt", "qwerty"),
    row("ru-ru",                             "ru", "йцукен"),
    row("ru-statica-3x5",                    "ru", "statica"),
    row("be-by",                             "be", "йцукен"),
    row("uk-ua",                             "uk", "йцукен"),
    row("sv-se",                             "sv", "qwerty"),
    row("cs-cz",                             "cs", "qwertz"),
    row("sl-si",                             "sl", "qwertz"),
    row("el-gr",                             "el", "greek"),
    row("nb-no",                             "nb", "qwerty"),
    row("he-il",                             "he", "hebrew"),
    row("nl-nl",                             "nl", "qwerty"),
    row("nl-be",                             "nl", "azerty"),
    row("hu-hu",                             "hu", "qwertz"),
    row("hy-am-phonetic",                    "hy", "hy-am-phonetic"),
    row("hy-am-typewriter",                  "hy", "hy-am-typewriter"),
    row("hy-am-eastern",                     "hy", "hy-am-eastern"),
    row("tr-tr-q",                           "tr", "qwerty"),
    row("tr-tr-f",                           "tr", "tr-tr-f"),
    row("en-canary",                         "en", "canary"),
    row("en-canary-matrix",                  "en", "canary-matrix"),
    row("en-norman",                         "en", "norman"),
    row("en-halmak",                         "en", "halmak"),
    row("ar-sa",                             "ar", "arabic"),
    row("ar-sa-102",                         "ar", "arabic"),
    row("fa-ir-legacy",                      "fa", "persian"),
    row("he-il-arkn",                        "he", "hebrew-arkn"),
    row("fa-ir",                             "fa", "persian"),
    row("en-engram",                         "en", "en-engram"),
    row("en-nerps",                          "en", "nerps"),
    row("en-nerps-matrix",                   "en", "nerps-matrix"),
    row("en-hands-down-neu",                 "en", "hands-down-neu"),
    row("en-sturdy",                         "en", "sturdy"),
    row("es-mx",                             "es", "qwerty"),
    row("pl-fwyr",                           "pl", "fwyr"),
    row("en-graphite",                       "en", "en-graphite"),
    row("en-graphite-angle-kp",              "en", "en-graphite-angle-kp"),
    row("th-th",                             "th", "th-th"),
    row("th-th-pat",                         "th", "th-th-pat"),
    row("th-th-man",                         "th", "th-th-man"),
    row("nb-no-kvikk",                       "nb", "kvikk"),
    row("nb-no-skarp",                       "nb", "skarp"),
    row("nb-no-dvorak",                      "nb", "dvorak"),
    row("ro-ro",                             "ro", "qwerty"),
    row("en-gallium",                        "en", "gallium"),
    row("en-gallium-matrix",                 "en", "gallium-matrix"),
    row("en-hands-down-promethium",          "en", "en-hands-down-promethium"),
    row("en-hands-down-promethium-inverted", "en", "en-hands-down-promethium"),
    row("en-aptv3",                          "en", "en-aptv3"),
    row("en_focal",                          "en", "focal"),
    row("de-adnw-but-xcv",                   "de", "de-adnw"),
    row("da-dk",                             "da", "qwerty"),
    row("en-enthium-v6",                     "en", "enthium"),
    row("en-enthium-v10",                    "en", "enthium"),
    row("en-enthium-v11",                    "en", "enthium"),
    row("en-enthium-v13",                    "en", "enthium"),
    row("en-enthium-v14",                    "en", "enthium"),
    row("en-night-matrix",                   "en", "night"),
    row("en-mtgap",                          "en", "MTGAP"),
    row("es-dvorak",                         "es", "dvorak"),
    row("es-latam-dvorak",                   "es", "dvorak"),
    row("et-ee",                             "et", "qwerty"),
    row("en-kuntem",                         "en", "kuntem"),
    row("br-chwerty-maths",                  "br", "chwerty"),
];

/// The layout with this id, if keybr.com has it. Ids are compared exactly,
/// as an export spells them.
pub fn lookup(id: &str) -> Option<&'static LayoutInfo> {
    LAYOUTS.iter().find(|l| l.id == id)
}

/// The family of the layout with this id. An id the table lacks (keybr.com's
/// `custom` layout, or one it added later) is a family of its own.
pub fn family_of(id: &str) -> &str {
    lookup(id).map_or(id, |l| l.family)
}

/// The family called `name`, ignoring case (`"mtgap"`, `"ЙЦУКЕН"`), in the
/// table's own spelling.
pub fn find_family(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    LAYOUTS
        .iter()
        .map(|l| l.family)
        .find(|f| f.to_lowercase() == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn table_has_every_keybr_layout_once() {
        assert_eq!(LAYOUTS.len(), 98);
        let ids: HashSet<&str> = LAYOUTS.iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), LAYOUTS.len(), "duplicate layout id");
        // Exports spell ids in lowercase ASCII, which `lookup` relies on.
        for l in LAYOUTS {
            assert!(
                l.id.bytes().all(|b| b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || b == b'-'
                    || b == b'_'),
                "{}",
                l.id
            );
        }
        assert_eq!(LAYOUTS[0].id, "en-us");
        assert_eq!(LAYOUTS[LAYOUTS.len() - 1].id, "br-chwerty-maths");
    }

    #[test]
    fn layouts_map_to_their_language_and_family() {
        let check = |id: &str, language: &str, family: &str| {
            let info = lookup(id).unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!((info.language, info.family), (language, family), "{id}");
        };
        check("en-us", "en", "qwerty");
        // The traps: an underscore, `uk` as a region and as a language,
        // Breton, Brazilian Portuguese.
        check("en_focal", "en", "focal");
        check("en-uk", "en", "qwerty");
        check("uk-ua", "uk", "йцукен");
        check("br-chwerty-maths", "br", "chwerty");
        check("pt-br", "pt", "qwerty");
        check("tr-tr-q", "tr", "qwerty");
        check("tr-tr-f", "tr", "tr-tr-f");
        check("de-de", "de", "qwertz");
        check("de-ch", "de", "qwertz");
        check("en-colemak", "en", "colemak");
        check("en-colemak-dh", "en", "colemak-dh");
        check("en-mtgap", "en", "MTGAP");
        assert_eq!(lookup("custom"), None);
        assert_eq!(lookup("us"), None);
        assert_eq!(lookup("EN-US"), None);
        assert_eq!(lookup("en"), None);
    }

    #[test]
    fn unknown_ids_are_their_own_family() {
        assert_eq!(family_of("en-uk"), "qwerty");
        assert_eq!(family_of("en-colemak"), "colemak");
        assert_eq!(family_of("custom"), "custom");
        assert_eq!(family_of("en-some-new-layout"), "en-some-new-layout");
    }

    #[test]
    fn families_are_found_in_any_case() {
        assert_eq!(find_family("colemak"), Some("colemak"));
        assert_eq!(find_family("Colemak"), Some("colemak"));
        assert_eq!(find_family("mtgap"), Some("MTGAP"));
        assert_eq!(find_family("ЙЦУКЕН"), Some("йцукен"));
        assert_eq!(find_family("tr-tr-f"), Some("tr-tr-f"));
        // A layout id is not a family name unless keybr.com uses it as one.
        assert_eq!(find_family("en-colemak"), None);
        assert_eq!(find_family("all"), None);
        assert_eq!(find_family(""), None);
    }
}
