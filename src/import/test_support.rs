//! Fixtures shared by the import tests.

use crate::engine::Language;
use crate::persistence::SavedStats;

use super::schema::{ExportRecord, HistogramEntry};
use super::{ImportSummary, LayoutChoice};

/// [`super::replay`] with the default layout choice, the family of the
/// latest session, which is all most tests need.
pub(super) fn replay(
    records: &[ExportRecord],
    target_cpm: f64,
    language: Language,
) -> (SavedStats, ImportSummary) {
    super::replay(records, target_cpm, language, LayoutChoice::Latest)
}

pub(super) fn entry(ch: char, hits: u32, misses: u32, time: u32) -> HistogramEntry {
    HistogramEntry {
        code_point: ch as u32,
        hit_count: hits,
        miss_count: misses,
        time_to_type: time,
    }
}

pub(super) fn record(samples: Vec<HistogramEntry>) -> ExportRecord {
    let length: u32 = 100;
    ExportRecord {
        layout: "en-us".to_string(),
        text_type: "generated".to_string(),
        time_stamp: "2024-05-01T12:00:00.000Z".to_string(),
        length,
        time: 30_000, // 100 chars / 30s = 200 cpm = 40 wpm
        errors: 5,
        speed: 0.0,
        histogram: samples,
    }
}

/// A three-letter English session in `layout`, stamped `time_stamp`.
pub(super) fn session(layout: &str, time_stamp: &str) -> ExportRecord {
    ExportRecord {
        layout: layout.to_string(),
        time_stamp: time_stamp.to_string(),
        ..record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
        ])
    }
}

/// Two QWERTY sessions and one Colemak session, with the latest of
/// them in the given layout and neither first nor last in the file.
pub(super) fn mixed_english(latest: &str) -> Vec<ExportRecord> {
    let other = if latest == "en-colemak" {
        "en-us"
    } else {
        "en-colemak"
    };
    vec![
        session("en-us", "2024-05-01T12:00:00.000Z"),
        session(latest, "2024-05-03T09:30:00.000Z"),
        session(other, "2024-05-02T23:59:59.999Z"),
        // Later still, but Turkish: not the English import's latest.
        session("tr-tr-f", "2024-06-01T00:00:00.000Z"),
        session("en-us", "2024-05-03T09:29:59.999Z"),
    ]
}
