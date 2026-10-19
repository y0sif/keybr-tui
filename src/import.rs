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

use crate::engine::{layout, Language, LetterScheduler};
use crate::metrics::KeyStats;
use crate::persistence::{
    local_day_utc_bounds_at, today_date_string_at, unix_now_secs, SavedKeyStats, SavedLessonResult,
    SavedStats,
};

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

/// Does an export record's timestamp fall inside the half-open window
/// `[start, end)`?
///
/// `time_stamp` is UTC and fixed width (`"2021-09-22T15:39:08.000Z"`), and so
/// are the bounds (`"YYYY-MM-DDTHH:MM:SS"`), so byte-lexicographic order is
/// chronological order and a plain string compare is a correct instant
/// compare. A record stamped exactly at `start` sorts after it (`".000Z"` is
/// a suffix) and is included; one stamped exactly at `end` sorts after `end`
/// and is excluded — which is what half-open means.
///
/// This replaces a `starts_with(local_date)` prefix match, which was wrong the
/// moment the date became local: the local day maps to a UTC window that
/// straddles two UTC dates for every zone except UTC itself.
fn timestamp_in_window(time_stamp: &str, start: &str, end: &str) -> bool {
    time_stamp >= start && time_stamp < end
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

/// The layout family of the language's most recent session. Export stamps
/// are fixed-width UTC (see `timestamp_in_window`), so the greatest string
/// is the latest; of equal stamps the one later in the file wins.
fn latest_family(records: &[ExportRecord], language: Language) -> Option<&str> {
    records
        .iter()
        .filter(|r| language.owns_layout(&r.layout))
        .max_by(|a, b| a.time_stamp.cmp(&b.time_stamp))
        .map(|r| layout::family_of(&r.layout))
}

/// Outcome counters and headline numbers for the post-import report.
#[derive(Debug, Default)]
pub struct ImportSummary {
    pub total: usize,
    pub imported: usize,
    pub skipped_layout: usize,
    /// The language's sessions typed in another layout family than the
    /// one imported.
    pub skipped_family: usize,
    pub skipped_invalid: usize,
    pub unlocked_letters: usize,
    pub focused_key: Option<char>,
    /// The layout family imported: `None` with `--layout all`, or when
    /// the language has no sessions to take a family from.
    pub family: Option<String>,
    /// Distinct layout ids among the imported sessions, in first-seen
    /// order.
    pub layouts: Vec<String>,
    /// Distinct families among the imported sessions, in first-seen order.
    /// More than one (only with `--layout all`) means their key timings
    /// were merged, which the report warns about.
    pub families: Vec<String>,
    /// Distinct families of the sessions counted in `skipped_family`.
    pub skipped_families: Vec<String>,
    pub total_practice_secs: u64,
    /// Mean WPM over the last (up to) 10 imported sessions.
    pub recent_wpm: Option<f64>,
}

/// Replay exported results in file order (the order is authoritative —
/// keybr never re-sorts) and produce the equivalent `SavedStats`.
pub fn replay(
    records: &[ExportRecord],
    target_cpm: f64,
    language: Language,
    choice: LayoutChoice,
) -> (SavedStats, ImportSummary) {
    // One clock reading for both derived values. Reading it twice (once for the
    // window, once for the date label) can straddle local midnight and stamp
    // day D's practice seconds with day D+1's date.
    let now = unix_now_secs();
    // The UTC window matching the current *local* day — records are stamped in
    // UTC while `today_date` is a local date, so they can only be compared
    // through this window. `None` on the clock-error path.
    let today_window = now.map(local_day_utc_bounds_at);
    // Empty string on that same clock-error path, exactly what
    // `today_date_string()` yields there.
    let today_date = now.map(today_date_string_at).unwrap_or_default();
    replay_with_window(
        records,
        target_cpm,
        today_window,
        today_date,
        language,
        choice,
    )
}

/// Core of [`replay`] with the "is this session today?" window and the local
/// date label injected, so the daily-goal accounting can be tested without the
/// wall clock or `TZ`. Both arguments must come from the same instant.
fn replay_with_window(
    records: &[ExportRecord],
    target_cpm: f64,
    today_window: Option<(String, String)>,
    today_date: String,
    language: Language,
    choice: LayoutChoice,
) -> (SavedStats, ImportSummary) {
    const HISTORY_CAP: usize = 50;
    const RECENT_TIMES_CAP: usize = 50;

    let family = match choice {
        LayoutChoice::Latest => latest_family(records, language),
        LayoutChoice::All => None,
        LayoutChoice::Family(family) => Some(family),
    };
    let mut summary = ImportSummary {
        total: records.len(),
        family: family.map(str::to_string),
        ..ImportSummary::default()
    };

    let mut keys: HashMap<char, KeyStats> = HashMap::new();
    let mut lesson_history: Vec<SavedLessonResult> = Vec::new();
    let mut last_time_stamp = String::new();
    let mut today_seconds: u64 = 0;

    for record in records {
        // Key stats belong to one language's alphabet; sessions typed in
        // another language can't be replayed into it.
        if !language.owns_layout(&record.layout) {
            summary.skipped_layout += 1;
            continue;
        }
        // keybr.com keeps one layout family's key stats at a time.
        let record_family = layout::family_of(&record.layout);
        if family.is_some_and(|f| f != record_family) {
            summary.skipped_family += 1;
            push_new(&mut summary.skipped_families, record_family);
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
                Some(c) if language.is_letter(c) => c,
                // Space, digits, punctuation, uppercase: keybr's letter set
                // is the language's lowercase alphabet only — these never
                // participate in key stats.
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

        // No window (clock before the epoch) → count nothing toward today,
        // matching the old `!today.is_empty()` guard on the same failure path.
        if let Some((start, end)) = &today_window {
            if timestamp_in_window(&record.time_stamp, start, end) {
                today_seconds += record.time / 1000;
            }
        }
        summary.total_practice_secs += record.time / 1000;
        last_time_stamp = record.time_stamp.clone();
        push_new(&mut summary.layouts, &record.layout);
        push_new(&mut summary.families, record_family);
        summary.imported += 1;
    }

    // Derive the unlocked set + focused key from the replayed stats.
    // `update()` unlocks at most one letter per call, so iterate until the
    // active set stops growing — same fixed point keybr reaches by deriving
    // the whole set in one pass.
    let mut scheduler = LetterScheduler::for_language(language);
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
        today_date,
        last_lesson: lesson_history.last().cloned(),
        lesson_history,
    };

    (saved, summary)
}

/// Append `value` unless `list` already has it, keeping first-seen order.
fn push_new(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_string());
    }
}

/// Why nothing was imported, for the `--import` error.
fn nothing_imported(language: Language, path: &Path, summary: &ImportSummary) -> String {
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
fn family_report(summary: &ImportSummary, choice: LayoutChoice) -> Vec<String> {
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

/// Full `--import` flow: parse the export, replay it, back up any existing
/// stats file, save, and print a report. Refuses to overwrite existing
/// stats unless `force` is set. `choice` picks the layout family
/// (`--layout`).
pub fn run_import(
    path: &Path,
    target_cpm: f64,
    force: bool,
    language: Language,
    choice: LayoutChoice,
) -> color_eyre::Result<()> {
    let stats_path = SavedStats::path(language)
        .ok_or_else(|| eyre!("could not determine the data directory"))?;
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

    let (saved, summary) = replay(&records, target_cpm, language, choice);
    if summary.imported == 0 {
        return Err(eyre!(nothing_imported(language, path, &summary)));
    }

    if stats_path.exists() {
        let backup = stats_path.with_extension("json.bak");
        std::fs::copy(&stats_path, &backup)
            .wrap_err_with(|| format!("could not back up stats to {}", backup.display()))?;
        println!("Backed up existing stats to {}", backup.display());
    }
    saved.save(language)?;

    println!(
        "Imported {} of {} sessions as {} ({} skipped: other layout, {} skipped: invalid).",
        summary.imported,
        summary.total,
        language.display_name(),
        summary.skipped_layout,
        summary.skipped_invalid
    );
    for line in family_report(&summary, choice) {
        println!("{line}");
    }
    println!(
        "Unlocked letters: {}/{}{} — derived against your {:.0} WPM target \
         (from --target-wpm or config.toml; changing it re-derives, like keybr.com).",
        summary.unlocked_letters,
        language.letters().len(),
        summary
            .focused_key
            .map(|c| format!(" (current focus: '{c}')"))
            .unwrap_or_default(),
        target_cpm / 5.0
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
    use crate::persistence::{datetime_string_from_unix_secs, today_date_string};

    /// [`super::replay`] with the default layout choice, the family of the
    /// latest session, which is all most tests need.
    fn replay(
        records: &[ExportRecord],
        target_cpm: f64,
        language: Language,
    ) -> (SavedStats, ImportSummary) {
        super::replay(records, target_cpm, language, LayoutChoice::Latest)
    }

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
        let (saved, summary) = replay(&records, 175.0, Language::English);

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
        let (saved, _) = replay(&records, 175.0, Language::English);
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
        let (saved, summary) = replay(&records, 175.0, Language::English);
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
        let (saved, _) = replay(&records, 175.0, Language::English);
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
        let (saved, summary) = replay(&records, 175.0, Language::English);
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
        let (saved, summary) = replay(&records, 175.0, Language::English);
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
        let (saved, summary) = replay(&records, 175.0, Language::English);
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
        let (saved, _) = replay(&[r], 175.0, Language::English);
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
        let (saved, summary) = replay(&records, 175.0, Language::English);
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
        let (saved, _) = replay(&[r], 175.0, Language::English);
        let last = saved.last_lesson.unwrap();
        assert!((last.accuracy - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fast_keys_everywhere_unlock_all_letters() {
        // One session where every letter is well under the 175-CPM target
        // time (342.86ms) — bestConfidence >= 1 for all 26, so the unlock
        // loop should walk the whole order and leave nothing locked.
        let samples: Vec<HistogramEntry> = ('a'..='z').map(|ch| entry(ch, 10, 0, 200)).collect();
        let (saved, summary) = replay(&[record(samples)], 175.0, Language::English);
        assert_eq!(summary.unlocked_letters, 26);
        assert_eq!(saved.unlocked_letters.len(), 26);
        // All learned by best → no boosted letter, exactly like keybr.
        assert_eq!(summary.focused_key, None);
    }

    #[test]
    fn slow_frontier_letter_blocks_further_unlocks_and_gets_focus() {
        // Starters fast, 't' (7th in UNLOCK_ORDER) slow: 't' unlocks as the
        // frontier letter but stays unlearned, so nothing past it unlocks
        // and it becomes the focused key.
        let mut samples: Vec<HistogramEntry> =
            "eniarl".chars().map(|ch| entry(ch, 10, 0, 200)).collect();
        samples.push(entry('t', 10, 0, 800));
        let (saved, summary) = replay(&[record(samples)], 175.0, Language::English);
        assert_eq!(saved.unlocked_letters.len(), 7);
        assert!(saved.unlocked_letters.contains(&'t'));
        assert_eq!(summary.focused_key, Some('t'));
    }

    #[test]
    fn no_records_yields_starter_set() {
        let (saved, summary) = replay(&[], 175.0, Language::English);
        assert_eq!(summary.imported, 0);
        assert_eq!(saved.unlocked_letters, vec!['e', 'n', 'i', 'a', 'r', 'l']);
        assert!(saved.keys.is_empty());
        assert!(saved.last_lesson.is_none());
    }

    #[test]
    fn record_east_of_utc_counts_toward_the_local_day_it_falls_in() {
        // Local zone UTC+6 → the local Jan 16 is [Jan 15 18:00Z, Jan 16 18:00Z).
        // A session at 20:30 UTC on Jan 15 is 02:30 local on Jan 16, so it must
        // count toward today. The old prefix match against the local date
        // "2025-01-16" would have missed it entirely.
        let (start, end) = ("2025-01-15T18:00:00", "2025-01-16T18:00:00");
        assert!(timestamp_in_window("2025-01-15T20:30:00.000Z", start, end));
    }

    #[test]
    fn record_west_of_utc_counts_toward_the_local_day_it_falls_in() {
        // Local zone UTC-6 → the local Jan 15 is [Jan 15 06:00Z, Jan 16 06:00Z).
        // 01:00 UTC on Jan 16 is 19:00 local on Jan 15, so it counts toward
        // today; the prefix match against "2025-01-15" would have missed it.
        let (start, end) = ("2025-01-15T06:00:00", "2025-01-16T06:00:00");
        assert!(timestamp_in_window("2025-01-16T01:00:00.000Z", start, end));
    }

    #[test]
    fn records_outside_the_window_do_not_count() {
        let (start, end) = ("2025-01-15T18:00:00", "2025-01-16T18:00:00");
        // Well before.
        assert!(!timestamp_in_window("2025-01-14T23:00:00.000Z", start, end));
        // Well after.
        assert!(!timestamp_in_window("2025-01-17T00:00:00.000Z", start, end));
        // One second before the window opens.
        assert!(!timestamp_in_window("2025-01-15T17:59:59.000Z", start, end));
    }

    #[test]
    fn window_is_half_open() {
        let (start, end) = ("2025-01-15T18:00:00", "2025-01-16T18:00:00");

        // A real export stamp is 24 bytes ("...T18:00:00.000Z") and a bound is
        // 19, so a record can never be byte-equal to a bound: the ".000Z"
        // suffix decides both of the cases below whichever way the comparison
        // operators point. They are the user-facing shape, so keep them...
        assert!(timestamp_in_window("2025-01-15T18:00:00.000Z", start, end));
        assert!(!timestamp_in_window("2025-01-16T18:00:00.000Z", start, end));
        assert!(timestamp_in_window("2025-01-16T17:59:59.999Z", start, end));

        // ...but only a bound-width value lands exactly ON a bound, which is
        // the one input that tells `>=` from `>` and `<` from `<=`. Without
        // these two asserts the operators are unpinned and the "half-open" in
        // this test's name is decorative.
        assert!(
            timestamp_in_window(start, start, end),
            "a value exactly at the start is inside the window (`>=`, not `>`)"
        );
        assert!(
            !timestamp_in_window(end, start, end),
            "a value exactly at the end is outside the window (`<`, not `<=`)"
        );
    }

    fn dated_record(time_stamp: &str, time_ms: u64) -> ExportRecord {
        let mut r = record(vec![
            entry('e', 10, 0, 400),
            entry('n', 10, 0, 400),
            entry('i', 10, 0, 400),
        ]);
        r.time_stamp = time_stamp.to_string();
        r.time = time_ms;
        r
    }

    #[test]
    fn replay_credits_only_sessions_inside_the_local_day_window() {
        // End-to-end through the replay loop, with the window injected so the
        // test depends on neither the wall clock nor `TZ`.
        // Local zone UTC+6, local "today" is Jan 16 → [Jan 15 18:00Z, Jan 16 18:00Z).
        let window = Some((
            "2025-01-15T18:00:00".to_string(),
            "2025-01-16T18:00:00".to_string(),
        ));
        let records = vec![
            // 20:30Z Jan 15 = 02:30 local Jan 16 → today. The pre-fix prefix
            // match against "2025-01-16" dropped exactly this record.
            dated_record("2025-01-15T20:30:00.000Z", 30_000),
            // Past the window: tomorrow locally.
            dated_record("2025-01-16T19:00:00.000Z", 60_000),
        ];
        let (saved, summary) = replay_with_window(
            &records,
            175.0,
            window,
            "2025-01-16".to_string(),
            Language::English,
            LayoutChoice::Latest,
        );
        assert_eq!(summary.imported, 2);
        // Only the in-window session counts toward the daily goal...
        assert_eq!(saved.today_seconds_practiced, 30);
        // ...while both still count toward all-time practice.
        assert_eq!(summary.total_practice_secs, 90);
    }

    #[test]
    fn replay_credits_nothing_to_today_without_a_window() {
        // Clock-error path: `unix_now_secs()` returns None, so `replay()` passes
        // down no window and an empty date, and nothing is credited to the
        // daily goal — same as the old `!today.is_empty()` guard did when
        // `today_date_string()` returned "".
        let records = vec![dated_record("2025-01-15T20:30:00.000Z", 30_000)];
        let (saved, summary) = replay_with_window(
            &records,
            175.0,
            None,
            String::new(),
            Language::English,
            LayoutChoice::Latest,
        );
        assert_eq!(summary.imported, 1);
        assert_eq!(saved.today_seconds_practiced, 0);
        assert_eq!(summary.total_practice_secs, 30);
    }

    #[test]
    fn replay_credits_a_session_stamped_now_through_the_live_clock() {
        // The only test that goes through the real `replay()` entry point, so
        // the only cover for its live-clock wiring. Break either derived value
        // in `replay()` — window to `None`, or the date label to a literal —
        // and one of the two asserts below fails; every other import test
        // injects both and so leaves those lines untested.
        //
        // `local_day_utc_bounds_at(now)` always contains `now`, so a record
        // stamped at the instant this test reads the clock is inside the window
        // `replay()` builds from its own (marginally later) reading. The one
        // way that fails is local midnight falling in the microseconds between
        // the two readings, which would also move `today_date_string()` below.
        let now = unix_now_secs().expect("system clock before the epoch");
        let stamp = format!("{}.000Z", datetime_string_from_unix_secs(now));
        let (saved, summary) = replay(&[dated_record(&stamp, 30_000)], 175.0, Language::English);

        assert_eq!(summary.imported, 1);
        assert!(
            saved.today_seconds_practiced > 0,
            "a session stamped at the current instant ({stamp}) must count \
             toward the daily goal, got {} seconds",
            saved.today_seconds_practiced
        );
        assert_eq!(
            saved.today_date,
            today_date_string(),
            "the seconds credited to today must be labelled with today's local date"
        );
    }

    #[test]
    fn saved_stats_roundtrip_through_app_restore() {
        // The import output must load through the same path the app uses.
        let samples: Vec<HistogramEntry> = ('a'..='z').map(|ch| entry(ch, 10, 0, 200)).collect();
        let (saved, _) = replay(&[record(samples)], 175.0, Language::English);
        let json = serde_json::to_string(&saved).unwrap();
        let reloaded: SavedStats = serde_json::from_str(&json).unwrap();
        assert_eq!(reloaded.version, 2);
        assert_eq!(reloaded.unlocked_letters.len(), 26);
        assert_eq!(reloaded.keys.len(), 26);
    }

    fn turkish_record(layout: &str, samples: Vec<HistogramEntry>) -> ExportRecord {
        ExportRecord {
            layout: layout.to_string(),
            ..record(samples)
        }
    }

    #[test]
    fn turkish_import_keeps_turkish_letters_and_skips_english_sessions() {
        let records = vec![
            turkish_record(
                "tr-tr-q",
                vec![
                    entry(' ', 20, 0, 300),
                    entry('ş', 10, 1, 400),
                    entry('ı', 12, 0, 350),
                    entry('i', 9, 2, 380),
                    entry('a', 30, 1, 300),
                    // 'w' is not a Turkish letter: never a key stat.
                    entry('w', 3, 0, 500),
                ],
            ),
            record(vec![
                entry('e', 44, 3, 826),
                entry('n', 23, 4, 508),
                entry('i', 8, 2, 616),
            ]),
        ];
        let (saved, summary) = replay(&records, 175.0, Language::Turkish);
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped_layout, 1);
        assert_eq!(summary.layouts, vec!["tr-tr-q".to_string()]);
        for c in ['ş', 'ı', 'i', 'a'] {
            assert!(saved.keys.contains_key(&c), "missing '{c}'");
        }
        assert!(!saved.keys.contains_key(&'w'));
        assert!(!saved.keys.contains_key(&' '));
        assert_eq!(saved.unlocked_letters, vec!['a', 'e', 'i', 'n', 'ı', 'r']);
        // Dotted and dotless i stay separate keys with their own counts.
        assert_eq!(saved.keys[&'ı'].attempts, 12);
        assert_eq!(saved.keys[&'i'].attempts, 9);
    }

    #[test]
    fn turkish_q_and_f_are_separate_families() {
        // keybr.com puts Turkish Q in the `qwerty` family and F in its own
        // `tr-tr-f`, so by default only the latest one's sessions import.
        let samples = || {
            vec![
                entry('a', 30, 1, 300),
                entry('e', 20, 1, 320),
                entry('ş', 10, 1, 400),
            ]
        };
        let at = |layout: &str, time_stamp: &str| ExportRecord {
            time_stamp: time_stamp.to_string(),
            ..turkish_record(layout, samples())
        };
        let records = vec![
            at("tr-tr-q", "2024-05-01T12:00:00.000Z"),
            at("tr-tr-f", "2024-05-03T12:00:00.000Z"),
            at("tr-tr-q", "2024-05-02T12:00:00.000Z"),
        ];
        let (_, summary) = replay(&records, 175.0, Language::Turkish);
        assert_eq!(summary.family.as_deref(), Some("tr-tr-f"));
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped_family, 2);
        assert_eq!(summary.skipped_layout, 0);
        assert_eq!(summary.skipped_families, vec!["qwerty".to_string()]);
        assert_eq!(summary.layouts, vec!["tr-tr-f".to_string()]);

        // `--layout all` merges both, and the report says so.
        let (_, summary) = super::replay(&records, 175.0, Language::Turkish, LayoutChoice::All);
        assert_eq!(summary.imported, 3);
        assert_eq!(summary.skipped_family, 0);
        assert_eq!(
            summary.layouts,
            vec!["tr-tr-q".to_string(), "tr-tr-f".to_string()]
        );
        assert_eq!(
            summary.families,
            vec!["qwerty".to_string(), "tr-tr-f".to_string()]
        );
        let report = family_report(&summary, LayoutChoice::All);
        assert!(
            report
                .iter()
                .any(|l| l.starts_with("Note: sessions from several layouts (tr-tr-q, tr-tr-f)")),
            "{report:?}"
        );
    }

    #[test]
    fn english_import_skips_turkish_sessions() {
        let records = vec![turkish_record(
            "tr-tr-q",
            vec![
                entry('a', 30, 1, 300),
                entry('e', 20, 1, 320),
                entry('ş', 10, 1, 400),
            ],
        )];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.imported, 0);
        assert_eq!(summary.skipped_layout, 1);
    }

    #[test]
    fn english_import_keeps_focal_sessions() {
        // keybr.com's Focal layout id is `en_focal`, with an underscore.
        let samples = || {
            vec![
                entry('e', 10, 0, 400),
                entry('n', 10, 0, 400),
                entry('i', 10, 0, 400),
            ]
        };
        let records = vec![
            ExportRecord {
                layout: "en_focal".to_string(),
                ..record(samples())
            },
            record(samples()),
        ];
        // Focal is a family of its own, so only `--layout all` takes both.
        let (_, summary) = super::replay(&records, 175.0, Language::English, LayoutChoice::All);
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.skipped_layout, 0);
        assert_eq!(
            summary.layouts,
            vec!["en_focal".to_string(), "en-us".to_string()]
        );
        assert_eq!(
            summary.families,
            vec!["focal".to_string(), "qwerty".to_string()]
        );
    }

    /// A three-letter English session in `layout`, stamped `time_stamp`.
    fn session(layout: &str, time_stamp: &str) -> ExportRecord {
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
    fn mixed_english(latest: &str) -> Vec<ExportRecord> {
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

    #[test]
    fn default_imports_the_family_of_the_latest_session() {
        let (_, summary) = replay(&mixed_english("en-colemak"), 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("colemak"));
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped_family, 3);
        assert_eq!(summary.skipped_layout, 1);
        assert_eq!(summary.skipped_families, vec!["qwerty".to_string()]);
        assert_eq!(summary.layouts, vec!["en-colemak".to_string()]);

        let (saved, summary) = replay(&mixed_english("en-us"), 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("qwerty"));
        assert_eq!(summary.imported, 3);
        assert_eq!(summary.skipped_family, 1);
        assert_eq!(summary.skipped_families, vec!["colemak".to_string()]);
        assert_eq!(saved.total_lessons, 3);
        assert_eq!(saved.keys[&'e'].attempts, 30);
        assert_eq!(
            summary.total,
            summary.imported
                + summary.skipped_layout
                + summary.skipped_family
                + summary.skipped_invalid
        );
    }

    #[test]
    fn equal_stamps_go_to_the_later_record() {
        let stamp = "2024-05-01T12:00:00.000Z";
        let records = vec![session("en-colemak", stamp), session("en-us", stamp)];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("qwerty"));
        let records = vec![session("en-us", stamp), session("en-colemak", stamp)];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("colemak"));
    }

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
        let (saved, summary) = super::replay(&records, 175.0, Language::English, LayoutChoice::All);
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
            let (_, summary) = super::replay(&records, 175.0, Language::English, choice);
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

    #[test]
    fn unknown_layout_ids_are_their_own_family() {
        let records = vec![
            session("en-us", "2024-05-01T12:00:00.000Z"),
            session("en-some-new-layout", "2024-05-02T12:00:00.000Z"),
        ];
        let (_, summary) = replay(&records, 175.0, Language::English);
        assert_eq!(summary.family.as_deref(), Some("en-some-new-layout"));
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped_families, vec!["qwerty".to_string()]);
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
        let (_, summary) = super::replay(&records, 175.0, Language::English, choice);
        assert_eq!(summary.imported, 0);
        assert_eq!(
            nothing_imported(Language::English, path, &summary),
            "no usable English records of layout family colemak in typing-data.json \
             (1 skipped: other layout, 1 skipped: other layout family, 0 skipped: invalid); \
             other families: qwerty (--layout all merges every family)"
        );
        // Without a family filter the message is the one it always was.
        let (_, summary) = super::replay(&records, 175.0, Language::German, LayoutChoice::All);
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
            let (_, summary) = super::replay(&records, 175.0, language, choice);
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
        let (_, summary) = super::replay(&records, 175.0, Language::English, choice);
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
        let (_, summary) = super::replay(&records, 175.0, Language::EnglishUk, choice);
        assert!(nothing_imported(Language::EnglishUk, path, &summary).ends_with(why));
        let (_, summary) = super::replay(&records, 175.0, Language::Croatian, choice);
        assert!(
            nothing_imported(Language::Croatian, path, &summary).ends_with(
                "keybr.com has no layouts of its own for Hrvatski, so an export \
                        records its sessions under another language's layout and they \
                        cannot be told apart"
            )
        );
    }

    #[test]
    fn german_import_takes_german_layouts() {
        let samples = || {
            vec![
                entry('e', 30, 1, 300),
                entry('n', 20, 1, 320),
                entry('ä', 10, 1, 400),
                entry('ß', 5, 0, 450),
            ]
        };
        let at = |layout: &str, time_stamp: &str| ExportRecord {
            layout: layout.to_string(),
            time_stamp: time_stamp.to_string(),
            ..record(samples())
        };
        let records = vec![
            at("de-de", "2024-05-01T12:00:00.000Z"),
            at("en-us", "2024-05-02T12:00:00.000Z"),
            at("de-ch", "2024-05-03T12:00:00.000Z"),
            at("fr-ch", "2024-05-04T12:00:00.000Z"),
        ];
        let (saved, summary) = replay(&records, 175.0, Language::German);
        assert_eq!(summary.family.as_deref(), Some("qwertz"));
        assert_eq!(summary.imported, 2);
        // {CH} French is `qwertz` too, but French: another language.
        assert_eq!(summary.skipped_layout, 2);
        assert_eq!(summary.skipped_family, 0);
        assert_eq!(saved.keys[&'ä'].attempts, 20);
        assert_eq!(saved.keys[&'ß'].attempts, 10);
        let (_, english) = replay(&records, 175.0, Language::English);
        assert_eq!(english.imported, 1);
    }
}
