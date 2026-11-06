//! The user-facing text of an import: why nothing was imported, and the
//! report's lines on layout families.

use std::path::Path;

use crate::engine::{layout, Language};

use super::{ImportSummary, LayoutChoice};

/// Why nothing was imported, for the `--import` error.
pub(super) fn nothing_imported(language: Language, path: &Path, summary: &ImportSummary) -> String {
    // Some languages (`en-gb`, `hr`) have no keybr.com layouts of their
    // own: keybr.com lets them be typed on another language's, and an
    // export names only the layout, so no session can be told to be theirs.
    let no_layouts = if layout::LAYOUTS.iter().any(|l| language.owns_layout(l.id)) {
        None
    } else {
        Some(format!(
            "; keybr.com has no layouts of its own for {}, so an export records its \
             sessions under another language's layout and they cannot be told apart",
            language.display_name()
        ))
    };
    let Some(family) = &summary.family else {
        return format!(
            "no usable {} records in {} ({} skipped: other layout, {} skipped: invalid){}",
            language.display_name(),
            path.display(),
            summary.skipped_layout,
            summary.skipped_invalid,
            no_layouts.unwrap_or_default()
        );
    };
    // A family with no layout of the language at all (English with
    // `--layout de-de`, which is `qwertz`) can never import anything, so
    // name the supported languages it is for. A family the table lacks
    // is an unknown layout id of the language itself (`layout::family_of`).
    let in_family = || layout::LAYOUTS.iter().filter(|l| l.family == family);
    let hint = if let Some(no_layouts) = no_layouts {
        no_layouts
    } else if in_family().next().is_some() && !in_family().any(|l| language.owns_layout(l.id)) {
        let others: Vec<Language> = Language::ALL
            .iter()
            .copied()
            .filter(|other| in_family().any(|l| other.owns_layout(l.id)))
            .collect();
        let suggestion = match others.as_slice() {
            [] => String::new(),
            [other] => format!(
                "; for {} use --language {}",
                other.display_name(),
                other.code()
            ),
            // Codes keep a family shared by many languages (`qwerty`) to
            // one short line.
            _ => format!(
                "; its layouts are for {} (pick one with --language)",
                others
                    .iter()
                    .map(|o| o.code())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        format!(
            "; layout family {family} has no {} layouts{suggestion}",
            language.display_name()
        )
    } else if summary.skipped_family > 0 {
        format!(
            "; other families: {} (--layout all merges every family)",
            summary.skipped_families.join(", ")
        )
    } else {
        String::new()
    };
    format!(
        "no usable {} records of layout family {family} in {} ({} skipped: other layout, \
         {} skipped: other layout family, {} skipped: invalid){hint}",
        language.display_name(),
        path.display(),
        summary.skipped_layout,
        summary.skipped_family,
        summary.skipped_invalid
    )
}

/// The report's lines on layout families: the family imported and how it
/// was chosen, the sessions left out for another family, and the note when
/// `--layout all` merged several.
pub(super) fn family_report(summary: &ImportSummary, choice: LayoutChoice) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match (&summary.family, choice) {
        (Some(family), LayoutChoice::Latest) => format!(
            "Layout family: {family}, from your latest session (--layout picks another, \
             or all)."
        ),
        (Some(family), _) => format!("Layout family: {family}, from --layout."),
        (None, _) => format!(
            "Layout family: all ({}), from --layout all.",
            summary.families.join(", ")
        ),
    });
    if summary.skipped_family > 0 {
        lines.push(format!(
            "Skipped {} sessions in other layout families ({}); --layout all merges them.",
            summary.skipped_family,
            summary.skipped_families.join(", ")
        ));
    }
    if summary.families.len() > 1 {
        lines.push(format!(
            "Note: sessions from several layouts ({}) were merged into one set of key stats.",
            summary.layouts.join(", ")
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::test_support::{mixed_english, replay, session};
    use crate::import::ExportRecord;

    #[test]
    fn several_layouts_of_one_family_merge_without_a_note() {
        // {US} and {UK} are both `qwerty`, as on keybr.com.
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            session("en-uk", "2024-05-02T12:00:00.000Z"),
        ];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.imported, 2);
        assert_eq!(
            summary.layouts,
            vec!["en-us".to_string(), "en-uk".to_string()]
        );
        assert_eq!(summary.families, vec!["qwerty".to_string()]);
        let report = family_report(&summary, LayoutChoice::Latest);
        assert_eq!(
            report,
            vec![
                "Layout family: qwerty, from your latest session (--layout picks another, or all)."
                    .to_string()
            ]
        );
    }

    #[test]
    fn layout_all_merges_every_family() {
        let records = mixed_english("en-colemak");
        let (saved, summary) =
            crate::import::replay(&records, 175.0, Language::English, LayoutChoice::All);
        assert_eq!(summary.family, None);
        assert_eq!(summary.imported, 4);
        assert_eq!(summary.skipped_family, 0);
        assert_eq!(summary.skipped_layout, 1);
        assert_eq!(saved.keys[&'e'].attempts, 40);
        assert_eq!(
            summary.families,
            vec!["qwerty".to_string(), "colemak".to_string()]
        );
        let report = family_report(&summary, LayoutChoice::All);
        assert_eq!(
            report[0],
            "Layout family: all (qwerty, colemak), from --layout all."
        );
        assert_eq!(
            report[1],
            "Note: sessions from several layouts (en-us, en-colemak) were merged into one set of key stats."
        );
        assert_eq!(report.len(), 2);
    }

    #[test]
    fn nothing_imported_names_the_family() {
        let path = Path::new("typing-data.json");
        // Only QWERTY sessions, but Colemak asked for.
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            session("tr-tr-q", "2024-05-02T12:00:00.000Z"),
        ];
        let choice = LayoutChoice::Family("colemak");
        let (_, summary) = crate::import::replay(&records, 175.0, Language::English, choice);
        assert_eq!(summary.imported, 0);
        assert_eq!(
            nothing_imported(Language::English, path, &summary),
            "no usable English records of layout family colemak in typing-data.json \
             (1 skipped: other layout, 1 skipped: other layout family, 0 skipped: invalid); \
             other families: qwerty (--layout all merges every family)"
        );
        // Without a family filter the message is the one it always was.
        let (_, summary) =
            crate::import::replay(&records, 175.0, Language::German, LayoutChoice::All);
        assert_eq!(
            nothing_imported(Language::German, path, &summary),
            "no usable Deutsch records in typing-data.json (2 skipped: other layout, 0 skipped: invalid)"
        );
        let (_, summary) = replay(&records, 175.0, Language::German);
        assert_eq!(summary.family, None);
        assert_eq!(
            nothing_imported(Language::German, path, &summary),
            "no usable Deutsch records in typing-data.json (2 skipped: other layout, 0 skipped: invalid)"
        );
    }

    #[test]
    fn nothing_imported_names_the_languages_of_a_foreign_family() {
        let path = Path::new("typing-data.json");
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            session("de-de", "2024-05-02T12:00:00.000Z"),
        ];
        let message = |language: Language, choice: &str| {
            let choice = LayoutChoice::parse(choice).unwrap();
            let (_, summary) = crate::import::replay(&records, 175.0, language, choice);
            assert_eq!(summary.imported, 0, "{choice:?}");
            nothing_imported(language, path, &summary)
        };
        // `qwertz` (German {DE}) has no English layout, and is the family
        // of several other languages, named by code in `Language::ALL` order.
        assert_eq!(
            message(Language::English, "de-de"),
            "no usable English records of layout family qwertz in typing-data.json \
             (1 skipped: other layout, 1 skipped: other layout family, 0 skipped: invalid); \
             layout family qwertz has no English layouts; \
             its layouts are for cs, de, fr, hu, sl (pick one with --language)"
        );
        // `qwerty` is English and Turkish, among many others.
        assert!(message(Language::German, "qwerty").ends_with(
            "; layout family qwerty has no Deutsch layouts; its layouts are for \
             en, da, es, et, fi, fr, it, nb, nl, pl, pt, ro, sv, tr (pick one with --language)"
        ));
        // `azerty` is French and Dutch.
        assert!(message(Language::English, "azerty").ends_with(
            "; layout family azerty has no English layouts; \
             its layouts are for fr, nl (pick one with --language)"
        ));

        // An unknown layout id is a family of the language's own, though
        // the table has no layout of it.
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            ExportRecord {
                length: 0,
                ..session("en-some-new-layout", "2024-05-02T12:00:00.000Z")
            },
        ];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("en-some-new-layout"));
        assert_eq!(
            nothing_imported(Language::English, path, &summary),
            "no usable English records of layout family en-some-new-layout in typing-data.json \
             (0 skipped: other layout, 1 skipped: other layout family, 1 skipped: invalid); \
             other families: qwerty (--layout all merges every family)"
        );
    }

    #[test]
    fn nothing_imported_names_the_one_language_of_a_foreign_family() {
        let path = Path::new("typing-data.json");
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            session("de-de", "2024-05-02T12:00:00.000Z"),
        ];
        // `greek` is Greek's alone, so the hint names it.
        let choice = LayoutChoice::parse("el-gr").unwrap();
        let (_, summary) = crate::import::replay(&records, 175.0, Language::English, choice);
        assert_eq!(summary.imported, 0);
        assert!(
            nothing_imported(Language::English, path, &summary).ends_with(
                "; layout family greek has no English layouts; for Ελληνικά use --language el"
            )
        );
    }

    #[test]
    fn nothing_imported_explains_a_language_without_layouts() {
        let without: Vec<Language> = Language::ALL
            .iter()
            .copied()
            .filter(|lang| !layout::LAYOUTS.iter().any(|l| lang.owns_layout(l.id)))
            .collect();
        assert_eq!(without, [Language::EnglishUk, Language::Croatian]);

        let path = Path::new("typing-data.json");
        let records = vec![
            session("en-uk", "2024-05-01T12:00:00.000Z"),
            session("sl-si", "2024-05-02T12:00:00.000Z"),
        ];
        let why = "keybr.com has no layouts of its own for English (UK), so an export \
                   records its sessions under another language's layout and they cannot \
                   be told apart";
        // No session of its own, so no latest family either.
        let (_, summary) = replay(&records, 175.0, Language::EnglishUk);
        assert_eq!(summary.imported, 0);
        assert_eq!(
            nothing_imported(Language::EnglishUk, path, &summary),
            format!(
                "no usable English (UK) records in typing-data.json \
                 (2 skipped: other layout, 0 skipped: invalid); {why}"
            )
        );
        // A family asked for gives the same reason, not that family's
        // languages.
        let choice = LayoutChoice::parse("qwerty").unwrap();
        let (_, summary) = crate::import::replay(&records, 175.0, Language::EnglishUk, choice);
        assert!(nothing_imported(Language::EnglishUk, path, &summary).ends_with(why));
        let (_, summary) = crate::import::replay(&records, 175.0, Language::Croatian, choice);
        assert!(
            nothing_imported(Language::Croatian, path, &summary).ends_with(
                "keybr.com has no layouts of its own for Hrvatski, so an export \
                        records its sessions under another language's layout and they \
                        cannot be told apart"
            )
        );
    }
}
