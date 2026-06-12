//! Import a keybr.com data export (`typing-data.json`) and rebuild the
//! learning state exactly as keybr.com would compute it.
//!
//! keybr.com stores no derived state — on every page load it replays the
//! raw result list through a per-key EMA (alpha = 0.1 on each session's
//! mean `timeToType`) and derives unlocks/focus from the smoothed times.
//! We do the same replay once and persist the outcome as `SavedStats`,
//! so the user continues in the TUI right where the website left off.

use std::collections::HashMap;
use std::path::Path;

use color_eyre::eyre::{eyre, WrapErr};
use serde::Deserialize;

use crate::engine::LetterScheduler;
use crate::metrics::KeyStats;
use crate::persistence::{today_date_string, SavedKeyStats, SavedLessonResult, SavedStats};

/// One practice session as serialized by keybr.com's "Download data" button
/// (`Result.toJSON()` in packages/keybr-result).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    /// Keyboard layout id, e.g. "en-us".
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

/// Validity thresholds mirroring keybr.com's `Result.isValid` and
/// `Histogram.validateSample`.
const MIN_LENGTH: u32 = 10;
const MIN_TIME_MS: u64 = 1000;
const MIN_COMPLEXITY: usize = 3;
const MIN_SPEED_CPM: f64 = 1.0;
const MIN_SAMPLE_MS: u32 = 40;
const MAX_SAMPLE_MS: u32 = 12_000;

/// keybr drops samples with implausible timing entirely; 0 is kept as
/// "counts, but contributes no timing".
fn sample_is_plausible(time_to_type: u32) -> bool {
    time_to_type == 0 || (MIN_SAMPLE_MS..=MAX_SAMPLE_MS).contains(&time_to_type)
}

/// Outcome counters and headline numbers for the post-import report.
#[derive(Debug, Default)]
pub struct ImportSummary {
    pub total: usize,
    pub imported: usize,
    pub skipped_layout: usize,
    pub skipped_invalid: usize,
    pub unlocked_letters: usize,
    pub focused_key: Option<char>,
    pub total_practice_secs: u64,
    /// Mean WPM over the last (up to) 10 imported sessions.
    pub recent_wpm: Option<f64>,
}

/// Replay exported results in file order (the order is authoritative —
/// keybr never re-sorts) and produce the equivalent `SavedStats`.
pub fn replay(records: &[ExportRecord], target_cpm: f64) -> (SavedStats, ImportSummary) {
    const HISTORY_CAP: usize = 50;
    const RECENT_TIMES_CAP: usize = 50;

    let mut summary = ImportSummary {
        total: records.len(),
        ..ImportSummary::default()
    };

    let today = today_date_string();
    let mut keys: HashMap<char, KeyStats> = HashMap::new();
    let mut lesson_history: Vec<SavedLessonResult> = Vec::new();
    let mut last_time_stamp = String::new();
    let mut today_seconds: u64 = 0;

    for record in records {
        // Key stats are grouped by layout family; this TUI is qwerty
        // English only, so anything outside en-* can't be replayed.
        if !record.layout.starts_with("en") {
            summary.skipped_layout += 1;
            continue;
        }

        let plausible: Vec<&HistogramEntry> = record
            .histogram
            .iter()
            .filter(|s| sample_is_plausible(s.time_to_type))
            .collect();

        let speed_cpm = if record.time > 0 {
            record.length as f64 * 60_000.0 / record.time as f64
        } else {
            0.0
        };
        let valid = record.length >= MIN_LENGTH
            && record.time >= MIN_TIME_MS
            && plausible.len() >= MIN_COMPLEXITY
            && speed_cpm >= MIN_SPEED_CPM;
        if !valid {
            summary.skipped_invalid += 1;
            continue;
        }

        for sample in plausible {
            let ch = match char::from_u32(sample.code_point) {
                Some(c) if c.is_ascii_lowercase() => c,
                // Space, digits, punctuation, uppercase: keybr's letter set
                // is a-z only — these never participate in key stats.
                _ => continue,
            };
            let stats = keys.entry(ch).or_default();
            stats.attempts += sample.hit_count;
            stats.errors += sample.miss_count;
            if sample.time_to_type > 0 {
                stats.add_smoothed_sample(sample.time_to_type as f64);
                stats.reaction_times_ms.push(sample.time_to_type as u64);
                if stats.reaction_times_ms.len() > RECENT_TIMES_CAP {
                    stats.reaction_times_ms.remove(0);
                }
            }
        }

        let errors = record.errors.min(record.length);
        lesson_history.push(SavedLessonResult {
            wpm: speed_cpm / 5.0,
            accuracy: (record.length - errors) as f64 / record.length as f64 * 100.0,
        });
        if lesson_history.len() > HISTORY_CAP {
            lesson_history.remove(0);
        }

        if record.time_stamp.starts_with(&today) && !today.is_empty() {
            today_seconds += record.time / 1000;
        }
        summary.total_practice_secs += record.time / 1000;
        last_time_stamp = record.time_stamp.clone();
        summary.imported += 1;
    }

    // Derive the unlocked set + focused key from the replayed stats.
    // `update()` unlocks at most one letter per call, so iterate until the
    // active set stops growing — same fixed point keybr reaches by deriving
    // the whole set in one pass.
    let mut scheduler = LetterScheduler::new();
    loop {
        let before = scheduler.active_keys.len();
        scheduler.update(&keys, target_cpm);
        if scheduler.active_keys.len() == before {
            break;
        }
    }
    summary.unlocked_letters = scheduler.active_keys.len();
    summary.focused_key = scheduler.focused_key;

    let recent = &lesson_history[lesson_history.len().saturating_sub(10)..];
    if !recent.is_empty() {
        summary.recent_wpm = Some(recent.iter().map(|r| r.wpm).sum::<f64>() / recent.len() as f64);
    }

    let saved = SavedStats {
        version: 2,
        keys: keys
            .into_iter()
            .map(|(ch, st)| {
                (
                    ch,
                    SavedKeyStats {
                        attempts: st.attempts,
                        errors: st.errors,
                        filtered_time_ms: st.filtered_time_ms,
                        best_filtered_time_ms: st.best_filtered_time_ms,
                        recent_times_ms: st.reaction_times_ms,
                    },
                )
            })
            .collect(),
        unlocked_letters: scheduler.active_keys,
        total_lessons: summary.imported as u32,
        last_session: last_time_stamp,
        today_seconds_practiced: today_seconds.min(u32::MAX as u64) as u32,
        today_minutes_practiced: None,
        today_date: today,
        last_lesson: lesson_history.last().cloned(),
        lesson_history,
    };

    (saved, summary)
}

/// Full `--import` flow: parse the export, replay it, back up any existing
/// stats file, save, and print a report. Refuses to overwrite existing
/// stats unless `force` is set.
pub fn run_import(path: &Path, target_cpm: f64, force: bool) -> color_eyre::Result<()> {
    let stats_path =
        SavedStats::path().ok_or_else(|| eyre!("could not determine the data directory"))?;
    if stats_path.exists() && !force {
        return Err(eyre!(
            "stats already exist at {} — re-run with --force to replace them \
             (a .bak backup will be kept)",
            stats_path.display()
        ));
    }

    let contents = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("could not read {}", path.display()))?;
    let records: Vec<ExportRecord> = serde_json::from_str(&contents)
        .wrap_err("not a keybr.com export — expected a JSON array of result records")?;

    let (saved, summary) = replay(&records, target_cpm);
    if summary.imported == 0 {
        return Err(eyre!(
            "no usable records in {} ({} skipped: other layout, {} skipped: invalid)",
            path.display(),
            summary.skipped_layout,
            summary.skipped_invalid
        ));
    }

    if stats_path.exists() {
        let backup = stats_path.with_extension("json.bak");
        std::fs::copy(&stats_path, &backup)
            .wrap_err_with(|| format!("could not back up stats to {}", backup.display()))?;
        println!("Backed up existing stats to {}", backup.display());
    }
    saved.save()?;

    println!(
        "Imported {} of {} sessions ({} skipped: other layout, {} skipped: invalid).",
        summary.imported, summary.total, summary.skipped_layout, summary.skipped_invalid
    );
    println!(
        "Unlocked letters: {}/26{}",
        summary.unlocked_letters,
        summary
            .focused_key
            .map(|c| format!(" (current focus: '{c}')"))
            .unwrap_or_default()
    );
    println!(
        "Total practice time: {:.1} hours across {} sessions.",
        summary.total_practice_secs as f64 / 3600.0,
        summary.imported
    );
    if let Some(wpm) = summary.recent_wpm {
        println!(
            "Recent speed: ~{wpm:.0} WPM (last {} sessions).",
            10.min(summary.imported)
        );
    }
    println!("Saved to {}", stats_path.display());
    println!("Run `keybr-tui` to continue where you left off.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ch: char, hits: u32, misses: u32, time: u32) -> HistogramEntry {
        HistogramEntry {
            code_point: ch as u32,
            hit_count: hits,
            miss_count: misses,
            time_to_type: time,
        }
    }

    fn record(samples: Vec<HistogramEntry>) -> ExportRecord {
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
    fn replay_single_record_seeds_ema_and_counts() {
        let records = vec![record(vec![
            entry('e', 44, 3, 826),
            entry('n', 23, 4, 508),
            entry('i', 8, 2, 616),
        ])];
        let (saved, summary) = replay(&records, 175.0);

        assert_eq!(summary.imported, 1);
        let e = &saved.keys[&'e'];
        assert_eq!(e.attempts, 44);
        assert_eq!(e.errors, 3);
        assert!((e.filtered_time_ms - 826.0).abs() < f64::EPSILON);
        assert!((e.best_filtered_time_ms - 826.0).abs() < f64::EPSILON);
    }

    #[test]
    fn replay_applies_ema_across_records_in_order() {
        let records = vec![
            record(vec![
                entry('e', 10, 0, 400),
                entry('n', 10, 0, 400),
                entry('i', 10, 0, 400),
            ]),
            record(vec![
                entry('e', 10, 0, 200),
                entry('n', 10, 0, 400),
                entry('i', 10, 0, 400),
            ]),
        ];
        let (saved, _) = replay(&records, 175.0);
        let e = &saved.keys[&'e'];
        // 0.1 * 200 + 0.9 * 400 = 380; best = min(400, 380) = 380
        assert!((e.filtered_time_ms - 380.0).abs() < 0.01);
        assert!((e.best_filtered_time_ms - 380.0).abs() < 0.01);
        assert_eq!(e.attempts, 20);
    }

    #[test]
    fn zero_time_to_type_counts_attempts_but_not_timing() {
        let records = vec![record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('b', 1, 1, 0), // keybr writes 0 when no valid timing exists
        ])];
        let (saved, summary) = replay(&records, 175.0);
        assert_eq!(summary.imported, 1);
        let b = &saved.keys[&'b'];
        assert_eq!(b.attempts, 1);
        assert_eq!(b.errors, 1);
        assert!((b.filtered_time_ms - 0.0).abs() < f64::EPSILON);
        assert!(b.recent_times_ms.is_empty());
    }

    #[test]
    fn implausible_samples_are_dropped_entirely() {
        let records = vec![record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
            entry('a', 10, 2, 20),     // < 40ms: dropped
            entry('r', 10, 2, 13_000), // > 12s: dropped
        ])];
        let (saved, _) = replay(&records, 175.0);
        assert!(!saved.keys.contains_key(&'a'));
        assert!(!saved.keys.contains_key(&'r'));
    }

    #[test]
    fn non_letter_code_points_are_ignored() {
        let records = vec![record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry(' ', 22, 1, 708), // space
            entry('A', 2, 0, 500),  // uppercase
            entry('(', 3, 0, 600),  // punctuation
        ])];
        let (saved, summary) = replay(&records, 175.0);
        assert_eq!(summary.imported, 1);
        assert_eq!(saved.keys.len(), 2);
        assert!(saved.keys.contains_key(&'e'));
        assert!(saved.keys.contains_key(&'n'));
    }

    #[test]
    fn invalid_records_are_skipped() {
        let mut too_short = record(vec![
            entry('e', 5, 0, 400),
            entry('n', 5, 0, 400),
            entry('i', 5, 0, 400),
        ]);
        too_short.length = 5; // < MIN_LENGTH

        let mut too_quick = record(vec![
            entry('e', 5, 0, 400),
            entry('n', 5, 0, 400),
            entry('i', 5, 0, 400),
        ]);
        too_quick.time = 500; // < MIN_TIME_MS

        // < 3 plausible samples once the implausible one is dropped
        let too_simple = record(vec![entry('e', 10, 0, 400), entry('n', 10, 0, 20)]);

        let records = vec![too_short, too_quick, too_simple];
        let (saved, summary) = replay(&records, 175.0);
        assert_eq!(summary.imported, 0);
        assert_eq!(summary.skipped_invalid, 3);
        assert!(saved.keys.is_empty());
    }

    #[test]
    fn non_english_layouts_are_skipped() {
        let mut arabic = record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
        ]);
        arabic.layout = "ar-sa".to_string();
        let records = vec![arabic];
        let (saved, summary) = replay(&records, 175.0);
        assert_eq!(summary.imported, 0);
        assert_eq!(summary.skipped_layout, 1);
        assert!(saved.keys.is_empty());
    }

    #[test]
    fn speed_is_recomputed_not_trusted() {
        let mut r = record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
        ]);
        r.speed = 9999.0; // lies
        r.length = 100;
        r.time = 30_000; // 200 cpm = 40 wpm
        let (saved, _) = replay(&[r], 175.0);
        let last = saved.last_lesson.unwrap();
        assert!((last.wpm - 40.0).abs() < 0.01, "got {}", last.wpm);
    }

    #[test]
    fn lesson_history_is_capped_at_50() {
        let records: Vec<ExportRecord> = (0..60)
            .map(|_| {
                record(vec![
                    entry('e', 10, 0, 400),
                    entry('n', 10, 0, 400),
                    entry('i', 10, 0, 400),
                ])
            })
            .collect();
        let (saved, summary) = replay(&records, 175.0);
        assert_eq!(summary.imported, 60);
        assert_eq!(saved.total_lessons, 60);
        assert_eq!(saved.lesson_history.len(), 50);
    }

    #[test]
    fn accuracy_reflects_record_errors() {
        let mut r = record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
        ]);
        r.length = 100;
        r.errors = 10;
        let (saved, _) = replay(&[r], 175.0);
        let last = saved.last_lesson.unwrap();
        assert!((last.accuracy - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fast_keys_everywhere_unlock_all_letters() {
        // One session where every letter is well under the 175-CPM target
        // time (342.86ms) — bestConfidence >= 1 for all 26, so the unlock
        // loop should walk the whole order and leave nothing locked.
        let samples: Vec<HistogramEntry> = ('a'..='z').map(|ch| entry(ch, 10, 0, 200)).collect();
        let (saved, summary) = replay(&[record(samples)], 175.0);
        assert_eq!(summary.unlocked_letters, 26);
        assert_eq!(saved.unlocked_letters.len(), 26);
        // All learned by best → focus falls back to current-weakest, which
        // is still some letter (never None once stats exist).
        assert!(summary.focused_key.is_some());
    }

    #[test]
    fn slow_frontier_letter_blocks_further_unlocks_and_gets_focus() {
        // Starters fast, 't' (7th in UNLOCK_ORDER) slow: 't' unlocks as the
        // frontier letter but stays unlearned, so nothing past it unlocks
        // and it becomes the focused key.
        let mut samples: Vec<HistogramEntry> =
            "eniarl".chars().map(|ch| entry(ch, 10, 0, 200)).collect();
        samples.push(entry('t', 10, 0, 800));
        let (saved, summary) = replay(&[record(samples)], 175.0);
        assert_eq!(saved.unlocked_letters.len(), 7);
        assert!(saved.unlocked_letters.contains(&'t'));
        assert_eq!(summary.focused_key, Some('t'));
    }

    #[test]
    fn no_records_yields_starter_set() {
        let (saved, summary) = replay(&[], 175.0);
        assert_eq!(summary.imported, 0);
        assert_eq!(saved.unlocked_letters, vec!['e', 'n', 'i', 'a', 'r', 'l']);
        assert!(saved.keys.is_empty());
        assert!(saved.last_lesson.is_none());
    }

    #[test]
    fn saved_stats_roundtrip_through_app_restore() {
        // The import output must load through the same path the app uses.
        let samples: Vec<HistogramEntry> = ('a'..='z').map(|ch| entry(ch, 10, 0, 200)).collect();
        let (saved, _) = replay(&[record(samples)], 175.0);
        let json = serde_json::to_string(&saved).unwrap();
        let reloaded: SavedStats = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.version, 2);
        assert_eq!(reloaded.unlocked_letters.len(), 26);
        assert_eq!(reloaded.keys.len(), 26);
    }
}
