//! The shape of a keybr.com export and the `--layout` choice of which of
//! its sessions to import.

use serde::Deserialize;

use crate::engine::layout;

/// One practice session as serialized by keybr.com's "Download data" button
/// (`Result.toJSON()` in packages/keybr-result).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    /// Keyboard layout id, e.g. "en-us" or "tr-tr-q".
    pub layout: String,
    /// "generated" | "natural" | "numbers" | "code". keybr feeds every
    /// text type into key stats, so we don't filter on it either.
    #[allow(dead_code)]
    pub text_type: String,
    /// ISO 8601, e.g. "2021-09-22T15:39:08.000Z".
    pub time_stamp: String,
    /// Characters typed.
    pub length: u32,
    /// Session duration in milliseconds.
    pub time: u64,
    /// Count of characters that had at least one typo.
    pub errors: u32,
    /// Chars per minute. Redundant — keybr always recomputes it from
    /// length/time, and so do we.
    #[serde(default)]
    #[allow(dead_code)]
    pub speed: f64,
    pub histogram: Vec<HistogramEntry>,
}

/// Per-character sample within one session.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistogramEntry {
    pub code_point: u32,
    pub hit_count: u32,
    pub miss_count: u32,
    /// Mean latency (ms) of this character's correct presses in the
    /// session. 0 means "no valid timing sample", NOT "instant".
    pub time_to_type: u32,
}

/// Which of the language's sessions an import replays (`--layout`).
///
/// keybr.com practices with the sessions of the current layout's family
/// only (`Lesson.filter`), so {US} and {UK} share key stats while Colemak
/// keeps its own. keybr-tui keeps stats per language, so the family is
/// picked among the language's sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutChoice {
    /// The family of the language's most recent session, the closest an
    /// export comes to keybr.com's current layout.
    #[default]
    Latest,
    /// Every family merged into one set of key stats (`--layout all`), as
    /// imports did before families.
    All,
    /// One family, in the layout table's spelling.
    Family(&'static str),
}

impl LayoutChoice {
    /// Parse a `--layout` value: `all`, a keybr.com layout id (meaning its
    /// family) or a family name, all ignoring case.
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("all") {
            return Ok(LayoutChoice::All);
        }
        if let Some(info) = layout::lookup(&value.to_lowercase()) {
            return Ok(LayoutChoice::Family(info.family));
        }
        layout::find_family(value)
            .map(LayoutChoice::Family)
            .ok_or_else(|| {
                "not a keybr.com layout or layout family (expected all, a layout id \
                 such as en-colemak, or a family such as colemak)"
                    .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::import::report::family_report;
    use crate::import::test_support::mixed_english;

    #[test]
    fn parses_real_export_shape() {
        let json = r#"[{
            "layout": "en-us",
            "textType": "generated",
            "timeStamp": "2021-09-22T15:39:08.000Z",
            "length": 127,
            "time": 101913,
            "errors": 18,
            "speed": 74.76965647169645,
            "histogram": [
                {"codePoint": 32, "hitCount": 22, "missCount": 1, "timeToType": 708},
                {"codePoint": 101, "hitCount": 44, "missCount": 3, "timeToType": 826},
                {"codePoint": 110, "hitCount": 23, "missCount": 4, "timeToType": 508}
            ]
        }]"#;
        let records: Vec<ExportRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].length, 127);
        assert_eq!(records[0].histogram[1].code_point, 101);
        assert_eq!(records[0].histogram[1].time_to_type, 826);
    }

    #[test]
    fn layout_flag_picks_a_family_by_name_or_layout_id() {
        for value in ["colemak", "COLEMAK", "en-colemak", " En-Colemak "] {
            assert_eq!(
                LayoutChoice::parse(value),
                Ok(LayoutChoice::Family("colemak")),
                "{value:?}"
            );
        }
        assert_eq!(LayoutChoice::parse("all"), Ok(LayoutChoice::All));
        assert_eq!(LayoutChoice::parse("ALL"), Ok(LayoutChoice::All));
        assert_eq!(
            LayoutChoice::parse("en-uk"),
            Ok(LayoutChoice::Family("qwerty"))
        );
        assert_eq!(
            LayoutChoice::parse("mtgap"),
            Ok(LayoutChoice::Family("MTGAP"))
        );
        assert_eq!(
            LayoutChoice::parse("ЙЦУКЕН"),
            Ok(LayoutChoice::Family("йцукен"))
        );
        for value in ["colemack", "us", "custom", "latest", ""] {
            assert!(LayoutChoice::parse(value).is_err(), "{value:?}");
        }
        // No layout id names a family other than its own, so reading a
        // value as an id first never shadows a family.
        for info in layout::LAYOUTS {
            if let Some(family) = layout::find_family(info.id) {
                assert_eq!(family, info.family, "{}", info.id);
            }
        }

        // Either spelling imports the Colemak sessions, though the latest
        // is QWERTY.
        let records = mixed_english("en-us");
        for value in ["colemak", "en-colemak"] {
            let choice = LayoutChoice::parse(value).unwrap();
            let (_, summary) = crate::import::replay(&records, 175.0, Language::English, choice);
            assert_eq!(summary.family.as_deref(), Some("colemak"), "{value}");
            assert_eq!(summary.imported, 1, "{value}");
            assert_eq!(summary.skipped_family, 3, "{value}");
            assert_eq!(summary.layouts, vec!["en-colemak".to_string()]);
            let report = family_report(&summary, choice);
            assert_eq!(report[0], "Layout family: colemak, from --layout.");
            assert_eq!(
                report[1],
                "Skipped 3 sessions in other layout families (qwerty); --layout all merges them."
            );
            assert_eq!(report.len(), 2);
        }
    }
}
