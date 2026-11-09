//! Golden regression tests.
//!
//! These pin today's exact observable output: the lesson text the app
//! shows, the scheduler's unlock and focus decisions, a keybr.com import
//! replay, and the on-disk formats of `stats.json` and `config.toml`. They
//! exist so a behavior-preserving refactor that drifts fails loudly.
//!
//! The expected values were captured from v0.3.1 by running these tests
//! and pasting what they print (`cargo test golden -- --nocapture` prints
//! every actual value as a ready-to-paste `const`). Never edit an expected
//! value to make a refactor pass: a diff here is a behavior change. Moving
//! code between modules may change the `use` lines below, nothing else.
//!
//! Every value asserted is deterministic: RNGs are seeded, clock-derived
//! fields are left out or overwritten, and maps are sorted before they are
//! formatted. Exact JSON text is only pinned for stats with a single key,
//! since `SavedStats::keys` is a `HashMap` and serializes in random order.

use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, ErrorMode};
use crate::config::{Config, ErrorModeSerde};
use crate::engine::filter::FocusRule;
use crate::engine::scheduler::forced_extra_letters;
use crate::engine::{Language, LetterFilter, LetterScheduler, WordGenerator};
use crate::events::AppEvent;
use crate::import::{replay, ExportRecord, ImportSummary, LayoutChoice};
use crate::metrics::KeyStats;
use crate::persistence::{SavedKeyStats, SavedLessonResult, SavedStats};
use crate::update::update;

/// The default target: 35 WPM, 175 CPM, so a key is learned below
/// ~342.857 ms.
const TARGET_WPM: u32 = 35;
const TARGET_CPM: f64 = 175.0;

/// Compare `actual` with the pinned lines, printing `actual` first as a
/// ready-to-paste `const` so a deliberate re-capture is copy and paste.
fn check(name: &str, actual: &str, expected: &[&str]) {
    println!("const {name}: &[&str] = &[");
    for line in actual.split('\n') {
        println!("    {line:?},");
    }
    println!("];");
    assert_eq!(actual, expected.join("\n"), "golden value {name} drifted");
}

// ---------------------------------------------------------------------------
// Fixed synthetic per-key stats
// ---------------------------------------------------------------------------

/// One key's practice history: the key, its error count, then its lessons,
/// each a list of hit latencies in ms. Built through `KeyStats`'s own
/// recording API, so the EMA is part of what is pinned.
type KeySpec = (char, u32, &'static [&'static [u64]]);

/// A profile: a language, its unlocked letters in saved order, and the
/// practice history of every key it has stats for (locked keys included).
struct State {
    language: Language,
    unlocked: &'static str,
    keys: &'static [KeySpec],
}

/// English, starter set only. `l` is unlocked but never typed, `r` and `i`
/// are slow, `o` is locked with a fast best and a slow relapse.
static EN_EARLY: State = State {
    language: Language::English,
    unlocked: "eniarl",
    keys: &[
        ('e', 1, &[&[210, 190, 230], &[200, 180]]),
        ('n', 0, &[&[260, 240], &[250]]),
        ('i', 2, &[&[360, 380], &[340]]),
        ('a', 0, &[&[300], &[280, 300]]),
        ('r', 3, &[&[520, 560], &[540, 500]]),
        ('o', 0, &[&[250], &[900, 950], &[1000]]),
    ],
};

/// English, starter set with every key learned: the next letter unlocks.
static EN_UNLOCK: State = State {
    language: Language::English,
    unlocked: "eniarl",
    keys: &[
        ('e', 1, &[&[210, 190, 230], &[200, 180]]),
        ('n', 0, &[&[260, 240], &[250]]),
        ('i', 0, &[&[300]]),
        ('a', 0, &[&[300], &[280, 300]]),
        ('r', 0, &[&[320]]),
        ('l', 0, &[&[330]]),
    ],
};

/// English, sixteen letters, every one typed: the slowest (`h`) is the
/// focus, `o` has relapsed but stays learned by its best, `m` is locked.
static EN_LATER: State = State {
    language: Language::English,
    unlocked: "eniarltosudycghp",
    keys: &[
        ('e', 1, &[&[210, 190, 230], &[200, 180]]),
        ('n', 0, &[&[260, 240], &[250]]),
        ('i', 0, &[&[300]]),
        ('a', 0, &[&[300], &[280, 300]]),
        ('r', 0, &[&[320]]),
        ('l', 0, &[&[330]]),
        ('t', 0, &[&[280, 300]]),
        ('o', 0, &[&[250], &[900, 950], &[1000]]),
        ('s', 0, &[&[310]]),
        ('u', 0, &[&[335]]),
        ('d', 0, &[&[290]]),
        ('y', 1, &[&[400, 420]]),
        ('c', 0, &[&[330]]),
        ('g', 0, &[&[450]]),
        ('h', 4, &[&[600, 620]]),
        ('p', 0, &[&[380]]),
        ('m', 0, &[&[250]]),
    ],
};

/// German starter set, every key typed, `l` slowest.
static DE_EARLY: State = State {
    language: Language::German,
    unlocked: "entirl",
    keys: &[
        ('e', 0, &[&[220]]),
        ('n', 0, &[&[260]]),
        ('t', 0, &[&[400]]),
        ('i', 0, &[&[300]]),
        ('r', 0, &[&[380]]),
        ('l', 1, &[&[700]]),
    ],
};

/// German, twenty-two letters including `ü` (slow) and `ä` (never typed);
/// `ß` is locked.
static DE_LATER: State = State {
    language: Language::German,
    unlocked: "entirlhascgumofdkbpzüä",
    keys: &[
        ('e', 0, &[&[230]]),
        ('n', 0, &[&[240]]),
        ('t', 0, &[&[250]]),
        ('i', 0, &[&[260]]),
        ('r', 0, &[&[270]]),
        ('l', 0, &[&[280]]),
        ('h', 0, &[&[290]]),
        ('a', 0, &[&[300]]),
        ('s', 0, &[&[310]]),
        ('c', 0, &[&[320]]),
        ('g', 0, &[&[330]]),
        ('u', 0, &[&[335]]),
        ('m', 0, &[&[225]]),
        ('o', 0, &[&[245]]),
        ('f', 0, &[&[255]]),
        ('d', 0, &[&[265]]),
        ('k', 0, &[&[275]]),
        ('b', 0, &[&[285]]),
        ('p', 0, &[&[295]]),
        ('z', 0, &[&[305]]),
        ('ü', 2, &[&[520, 480]]),
        ('ß', 0, &[&[300]]),
    ],
};

/// Turkish starter set: dotted `i` slow, dotless `ı` slower, `r` never
/// typed.
static TR_EARLY: State = State {
    language: Language::Turkish,
    unlocked: "aeinır",
    keys: &[
        ('a', 0, &[&[240]]),
        ('e', 0, &[&[260]]),
        ('i', 0, &[&[420]]),
        ('n', 0, &[&[300]]),
        ('ı', 1, &[&[650]]),
    ],
};

/// Turkish, fourteen letters, `ü` slowest; `ş` is locked.
static TR_LATER: State = State {
    language: Language::Turkish,
    unlocked: "aeinırlutmküoy",
    keys: &[
        ('a', 0, &[&[240]]),
        ('e', 0, &[&[250]]),
        ('i', 0, &[&[260]]),
        ('n', 0, &[&[270]]),
        ('ı', 0, &[&[280]]),
        ('r', 0, &[&[290]]),
        ('l', 0, &[&[300]]),
        ('u', 0, &[&[310]]),
        ('t', 0, &[&[320]]),
        ('m', 0, &[&[330]]),
        ('k', 0, &[&[335]]),
        ('ü', 0, &[&[560]]),
        ('o', 0, &[&[300]]),
        ('y', 0, &[&[390]]),
        ('ş', 0, &[&[300]]),
    ],
};

/// Russian starter set: `и` slow, `е` never typed.
static RU_EARLY: State = State {
    language: Language::Russian,
    unlocked: "атиоен",
    keys: &[
        ('а', 0, &[&[250]]),
        ('т', 0, &[&[280]]),
        ('и', 2, &[&[480, 500]]),
        ('о', 0, &[&[300]]),
        ('н', 0, &[&[330]]),
    ],
};

fn build_stats(specs: &[KeySpec]) -> HashMap<char, KeyStats> {
    let mut map: HashMap<char, KeyStats> = HashMap::new();
    for &(ch, errors, lessons) in specs {
        let stats = map.entry(ch).or_default();
        for _ in 0..errors {
            stats.record_error();
        }
        for lesson in lessons {
            for &t in *lesson {
                stats.record_hit(t);
            }
            stats.finish_lesson();
        }
    }
    map
}

/// The state as the stats file `main` would load it from.
fn saved_stats(state: &State) -> SavedStats {
    let keys = build_stats(state.keys)
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
        .collect();
    SavedStats {
        version: 2,
        keys,
        unlocked_letters: state.unlocked.chars().collect(),
        total_lessons: 12,
        last_session: "1700000000".to_string(),
        today_seconds_practiced: 0,
        today_minutes_practiced: None,
        // Never today, so the restore never depends on the clock.
        today_date: "2001-01-01".to_string(),
        last_lesson: None,
        lesson_history: Vec::new(),
    }
}

/// One key's stats on one line, every float at fixed precision.
fn key_line(ch: char, st: &KeyStats) -> String {
    let fmt_wpm = |w: Option<f64>| w.map_or("-".to_string(), |w| format!("{w:.3}"));
    format!(
        "{ch} attempts={} errors={} filtered={:.4} best={:.4} conf={:.6} best_conf={:.6} \
         wpm={} best_wpm={} recent={:?}",
        st.attempts,
        st.errors,
        st.filtered_time_ms,
        st.best_filtered_time_ms,
        st.confidence(TARGET_CPM),
        st.best_confidence(TARGET_CPM),
        fmt_wpm(st.wpm()),
        fmt_wpm(st.best_wpm()),
        st.reaction_times_ms,
    )
}

fn sorted_keys<V>(map: &HashMap<char, V>) -> Vec<char> {
    let mut keys: Vec<char> = map.keys().copied().collect();
    keys.sort_unstable();
    keys
}

fn rule_str(rule: Option<FocusRule>) -> String {
    rule.map_or("-".to_string(), |r| r.config_value())
}

// ---------------------------------------------------------------------------
// 1. Lesson text, through the app's launch path
// ---------------------------------------------------------------------------

/// One lesson: a profile, the config.toml settings that matter, the seed
/// for the generator, and the focus the app is expected to resolve the
/// config's pins to (`None` for the scheduler's auto pick).
struct LessonCase {
    state: &'static State,
    natural_words: bool,
    fragment_length: usize,
    focus_letter: Option<char>,
    focus_pattern: Option<&'static str>,
    seed: u32,
    expected_pin: Option<FocusRule>,
}

impl LessonCase {
    fn config(&self) -> Config {
        Config {
            natural_words: self.natural_words,
            fragment_length: self.fragment_length,
            focus_letter: self.focus_letter,
            focus_pattern: self.focus_pattern.map(str::to_string),
            language: self.state.language.code().to_string(),
            ..Config::default()
        }
    }
}

/// Launch the way `main` does (restore the saved stats, then apply
/// config.toml), with the time-seeded generator swapped for a seeded one.
fn launch(state: &State, config: &Config) -> App {
    let mut app = App::new_with_language(
        config.target_wpm,
        ErrorMode::ForgiveMistakes,
        Some(saved_stats(state)),
        state.language,
    );
    app.apply_config(config);
    app
}

/// The same lesson straight through the engine: scheduler, filter,
/// generator. A cross-check that keeps working however `App` is split.
fn engine_lesson(case: &LessonCase) -> (Vec<char>, Option<FocusRule>, String) {
    let stats = build_stats(case.state.keys);
    let mut scheduler = LetterScheduler::for_language(case.state.language);
    assert!(scheduler.restore_active_keys(case.state.unlocked.chars().collect()));
    // The app runs the scheduler once when it is built and once more when
    // config.toml is applied.
    scheduler.update(&stats, TARGET_CPM);
    scheduler.update(&stats, TARGET_CPM);
    let focus = case
        .expected_pin
        .or(scheduler.focused_key.map(FocusRule::Key));
    let filter = LetterFilter::new(&scheduler.active_keys, focus);
    let mut generator = WordGenerator::with_seed_for(case.seed, case.state.language);
    generator.set_natural_words(case.natural_words);
    let text = generator.generate_fragment(&filter, case.fragment_length);
    (scheduler.active_keys, focus, text)
}

fn lesson_report(case: &LessonCase) -> String {
    let language = case.state.language;
    let mut app = launch(case.state, &case.config());
    app.generator = WordGenerator::with_seed_for(case.seed, language);
    app.start_next_lesson();

    let (engine_active, engine_focus, engine_text) = engine_lesson(case);
    assert_eq!(app.scheduler.active_keys, engine_active, "active keys");
    assert_eq!(app.effective_focus(), engine_focus, "effective focus");
    assert_eq!(app.generated_text, engine_text, "app and engine lessons");

    let active: String = app.scheduler.active_keys.iter().collect();
    let drills: Vec<String> = app
        .available_patterns()
        .iter()
        .map(FocusRule::config_value)
        .collect();
    let focus = app.effective_focus();
    format!(
        "language: {}\nactive: {active} ({})\nauto focus: {}\nfocus: {} label={} pinned={}\n\
         drills: [{}]\nchars: {}\ntext: {}",
        language.code(),
        language.upper_str(&active),
        rule_str(app.scheduler.focused_key.map(FocusRule::Key)),
        rule_str(focus),
        focus.map_or("-".to_string(), |r| r.label_in(language)),
        app.focus_is_pinned(),
        drills.join(","),
        app.generated_text.chars().count(),
        app.generated_text,
    )
}

const fn case(state: &'static State, seed: u32) -> LessonCase {
    LessonCase {
        state,
        natural_words: true,
        fragment_length: 100,
        focus_letter: None,
        focus_pattern: None,
        seed,
        expected_pin: None,
    }
}

#[test]
fn golden_lesson_en_early_phonetic() {
    let lesson = LessonCase {
        natural_words: false,
        ..case(&EN_EARLY, 1)
    };
    check(
        "LESSON_EN_EARLY_PHONETIC",
        &lesson_report(&lesson),
        LESSON_EN_EARLY_PHONETIC,
    );
}

#[test]
fn golden_lesson_en_early_natural() {
    let lesson = case(&EN_EARLY, 2);
    check(
        "LESSON_EN_EARLY_NATURAL",
        &lesson_report(&lesson),
        LESSON_EN_EARLY_NATURAL,
    );
}

#[test]
fn golden_lesson_en_unlock_natural() {
    let lesson = case(&EN_UNLOCK, 3);
    check(
        "LESSON_EN_UNLOCK_NATURAL",
        &lesson_report(&lesson),
        LESSON_EN_UNLOCK_NATURAL,
    );
}

#[test]
fn golden_lesson_en_later_natural() {
    let lesson = case(&EN_LATER, 4);
    check(
        "LESSON_EN_LATER_NATURAL",
        &lesson_report(&lesson),
        LESSON_EN_LATER_NATURAL,
    );
}

#[test]
fn golden_lesson_en_later_phonetic() {
    let lesson = LessonCase {
        natural_words: false,
        ..case(&EN_LATER, 4)
    };
    check(
        "LESSON_EN_LATER_PHONETIC",
        &lesson_report(&lesson),
        LESSON_EN_LATER_PHONETIC,
    );
}

#[test]
fn golden_lesson_en_manual_focus_letter() {
    // Hand-edited uppercase, which the app lowercases.
    let lesson = LessonCase {
        fragment_length: 60,
        focus_letter: Some('Y'),
        expected_pin: Some(FocusRule::Key('y')),
        ..case(&EN_LATER, 5)
    };
    check(
        "LESSON_EN_MANUAL_FOCUS_LETTER",
        &lesson_report(&lesson),
        LESSON_EN_MANUAL_FOCUS_LETTER,
    );
}

#[test]
fn golden_lesson_en_drill_suffix() {
    // A usable pattern pin outranks the letter pin.
    let lesson = LessonCase {
        focus_letter: Some('y'),
        focus_pattern: Some("-ing"),
        expected_pin: Some(FocusRule::Suffix("ing")),
        ..case(&EN_LATER, 6)
    };
    check(
        "LESSON_EN_DRILL_SUFFIX",
        &lesson_report(&lesson),
        LESSON_EN_DRILL_SUFFIX,
    );
}

#[test]
fn golden_lesson_en_drill_bigram_phonetic() {
    // A drill reads the dictionary even with natural words off.
    let lesson = LessonCase {
        natural_words: false,
        focus_pattern: Some("tr"),
        expected_pin: Some(FocusRule::Contains("tr")),
        ..case(&EN_LATER, 7)
    };
    check(
        "LESSON_EN_DRILL_BIGRAM_PHONETIC",
        &lesson_report(&lesson),
        LESSON_EN_DRILL_BIGRAM_PHONETIC,
    );
}

#[test]
fn golden_lesson_de_early_phonetic() {
    let lesson = LessonCase {
        natural_words: false,
        ..case(&DE_EARLY, 8)
    };
    check(
        "LESSON_DE_EARLY_PHONETIC",
        &lesson_report(&lesson),
        LESSON_DE_EARLY_PHONETIC,
    );
}

#[test]
fn golden_lesson_de_later_natural() {
    let lesson = case(&DE_LATER, 9);
    check(
        "LESSON_DE_LATER_NATURAL",
        &lesson_report(&lesson),
        LESSON_DE_LATER_NATURAL,
    );
}

#[test]
fn golden_lesson_tr_early_pinned_dotless_i() {
    // Uppercase `I` is dotless `ı` under Turkish casing.
    let lesson = LessonCase {
        focus_letter: Some('I'),
        expected_pin: Some(FocusRule::Key('ı')),
        ..case(&TR_EARLY, 10)
    };
    check(
        "LESSON_TR_EARLY_PINNED_DOTLESS_I",
        &lesson_report(&lesson),
        LESSON_TR_EARLY_PINNED_DOTLESS_I,
    );
}

#[test]
fn golden_lesson_tr_later_natural() {
    let lesson = case(&TR_LATER, 11);
    check(
        "LESSON_TR_LATER_NATURAL",
        &lesson_report(&lesson),
        LESSON_TR_LATER_NATURAL,
    );
}

#[test]
fn golden_lesson_ru_early_natural() {
    let lesson = case(&RU_EARLY, 12);
    check(
        "LESSON_RU_EARLY_NATURAL",
        &lesson_report(&lesson),
        LESSON_RU_EARLY_NATURAL,
    );
}

// ---------------------------------------------------------------------------
// 2. Scheduler and per-key confidence
// ---------------------------------------------------------------------------

/// Restore the profile's letters, run one scheduler update, and report the
/// unlocked set, the focus and every key's stats.
fn scheduler_report(state: &State, alphabet_size: f64) -> String {
    let language = state.language;
    let stats = build_stats(state.keys);
    let mut scheduler = LetterScheduler::for_language(language);
    let restored = scheduler.restore_active_keys(state.unlocked.chars().collect());
    scheduler.alphabet_size = alphabet_size;
    scheduler.update(&stats, TARGET_CPM);
    let mut out = format!(
        "language: {} restored={restored} alphabet_size={alphabet_size:.2} forced_extra={}\n\
         active: {}\nfocus: {}",
        language.code(),
        forced_extra_letters(language, alphabet_size),
        scheduler.active_keys.iter().collect::<String>(),
        scheduler
            .focused_key
            .map_or("-".to_string(), |c| c.to_string()),
    );
    for ch in sorted_keys(&stats) {
        out.push('\n');
        out.push_str(&key_line(ch, &stats[&ch]));
    }
    out
}

#[test]
fn golden_scheduler_en_early() {
    check(
        "SCHEDULER_EN_EARLY",
        &scheduler_report(&EN_EARLY, 0.0),
        SCHEDULER_EN_EARLY,
    );
}

#[test]
fn golden_scheduler_en_early_alphabet_size() {
    check(
        "SCHEDULER_EN_EARLY_ALPHABET_SIZE",
        &scheduler_report(&EN_EARLY, 0.5),
        SCHEDULER_EN_EARLY_ALPHABET_SIZE,
    );
}

#[test]
fn golden_scheduler_en_unlock() {
    check(
        "SCHEDULER_EN_UNLOCK",
        &scheduler_report(&EN_UNLOCK, 0.0),
        SCHEDULER_EN_UNLOCK,
    );
}

#[test]
fn golden_scheduler_en_later() {
    check(
        "SCHEDULER_EN_LATER",
        &scheduler_report(&EN_LATER, 0.0),
        SCHEDULER_EN_LATER,
    );
}

#[test]
fn golden_scheduler_de_later() {
    check(
        "SCHEDULER_DE_LATER",
        &scheduler_report(&DE_LATER, 0.0),
        SCHEDULER_DE_LATER,
    );
}

#[test]
fn golden_scheduler_tr_early() {
    check(
        "SCHEDULER_TR_EARLY",
        &scheduler_report(&TR_EARLY, 0.0),
        SCHEDULER_TR_EARLY,
    );
}

#[test]
fn golden_scheduler_ru_early() {
    check(
        "SCHEDULER_RU_EARLY",
        &scheduler_report(&RU_EARLY, 0.0),
        SCHEDULER_RU_EARLY,
    );
}

// ---------------------------------------------------------------------------
// 3. keybr.com import replay
// ---------------------------------------------------------------------------

/// A small keybr.com export. In file order: a Colemak session (another
/// layout family), a US session with a space, an uppercase letter, a
/// zero-time sample and an implausibly fast one, a UK session (same
/// family as US) with no `speed` field, an invalid session (too short), a
/// German session, and a final US session with more errors than
/// characters and an implausibly slow sample. Every stamp is years before
/// any clock this runs on, so nothing counts toward today.
const EXPORT_JSON: &str = r#"[
  {"layout": "en-colemak", "textType": "generated", "timeStamp": "2021-02-01T08:00:00.000Z",
   "length": 100, "time": 30000, "errors": 4, "speed": 200.0,
   "histogram": [
     {"codePoint": 101, "hitCount": 20, "missCount": 1, "timeToType": 180},
     {"codePoint": 110, "hitCount": 15, "missCount": 0, "timeToType": 190},
     {"codePoint": 105, "hitCount": 12, "missCount": 2, "timeToType": 200},
     {"codePoint": 97, "hitCount": 10, "missCount": 0, "timeToType": 210}]},
  {"layout": "en-us", "textType": "generated", "timeStamp": "2021-03-01T10:00:00.000Z",
   "length": 120, "time": 40000, "errors": 6, "speed": 180.0,
   "histogram": [
     {"codePoint": 32, "hitCount": 25, "missCount": 0, "timeToType": 150},
     {"codePoint": 101, "hitCount": 30, "missCount": 1, "timeToType": 250},
     {"codePoint": 110, "hitCount": 18, "missCount": 0, "timeToType": 280},
     {"codePoint": 105, "hitCount": 14, "missCount": 2, "timeToType": 300},
     {"codePoint": 97, "hitCount": 16, "missCount": 0, "timeToType": 320},
     {"codePoint": 114, "hitCount": 10, "missCount": 1, "timeToType": 300},
     {"codePoint": 108, "hitCount": 5, "missCount": 1, "timeToType": 0},
     {"codePoint": 69, "hitCount": 2, "missCount": 0, "timeToType": 400},
     {"codePoint": 116, "hitCount": 3, "missCount": 0, "timeToType": 30}]},
  {"layout": "en-uk", "textType": "natural", "timeStamp": "2021-03-02T09:30:00.000Z",
   "length": 150, "time": 45000, "errors": 3,
   "histogram": [
     {"codePoint": 101, "hitCount": 35, "missCount": 0, "timeToType": 230},
     {"codePoint": 110, "hitCount": 20, "missCount": 1, "timeToType": 260},
     {"codePoint": 105, "hitCount": 15, "missCount": 0, "timeToType": 290},
     {"codePoint": 97, "hitCount": 18, "missCount": 0, "timeToType": 300},
     {"codePoint": 114, "hitCount": 12, "missCount": 2, "timeToType": 380},
     {"codePoint": 108, "hitCount": 8, "missCount": 0, "timeToType": 350},
     {"codePoint": 116, "hitCount": 6, "missCount": 1, "timeToType": 500}]},
  {"layout": "en-us", "textType": "generated", "timeStamp": "2021-03-03T12:00:00.000Z",
   "length": 5, "time": 2000, "errors": 0, "speed": 150.0,
   "histogram": [
     {"codePoint": 101, "hitCount": 2, "missCount": 0, "timeToType": 100},
     {"codePoint": 110, "hitCount": 2, "missCount": 0, "timeToType": 100},
     {"codePoint": 105, "hitCount": 1, "missCount": 0, "timeToType": 100}]},
  {"layout": "de-de", "textType": "generated", "timeStamp": "2021-03-04T12:00:00.000Z",
   "length": 100, "time": 30000, "errors": 2, "speed": 200.0,
   "histogram": [
     {"codePoint": 101, "hitCount": 20, "missCount": 0, "timeToType": 200},
     {"codePoint": 110, "hitCount": 15, "missCount": 0, "timeToType": 200},
     {"codePoint": 116, "hitCount": 12, "missCount": 0, "timeToType": 200}]},
  {"layout": "en-us", "textType": "generated", "timeStamp": "2021-03-05T20:00:00.000Z",
   "length": 200, "time": 50000, "errors": 250, "speed": 240.0,
   "histogram": [
     {"codePoint": 101, "hitCount": 40, "missCount": 0, "timeToType": 200},
     {"codePoint": 110, "hitCount": 25, "missCount": 0, "timeToType": 210},
     {"codePoint": 105, "hitCount": 20, "missCount": 1, "timeToType": 220},
     {"codePoint": 97, "hitCount": 22, "missCount": 0, "timeToType": 230},
     {"codePoint": 114, "hitCount": 15, "missCount": 0, "timeToType": 240},
     {"codePoint": 108, "hitCount": 12, "missCount": 0, "timeToType": 250},
     {"codePoint": 116, "hitCount": 10, "missCount": 0, "timeToType": 260},
     {"codePoint": 111, "hitCount": 9, "missCount": 0, "timeToType": 270},
     {"codePoint": 115, "hitCount": 4, "missCount": 0, "timeToType": 20000}]}
]"#;

fn lesson_results(results: &[SavedLessonResult]) -> String {
    results
        .iter()
        .map(|r| format!("{:.4}/{:.4}", r.wpm, r.accuracy))
        .collect::<Vec<_>>()
        .join(" ")
}

fn summary_report(summary: &ImportSummary) -> String {
    format!(
        "total={} imported={} skipped_layout={} skipped_family={} skipped_invalid={}\n\
         unlocked_letters={} focused_key={:?} family={:?}\n\
         layouts={:?} families={:?} skipped_families={:?}\n\
         total_practice_secs={} recent_wpm={}",
        summary.total,
        summary.imported,
        summary.skipped_layout,
        summary.skipped_family,
        summary.skipped_invalid,
        summary.unlocked_letters,
        summary.focused_key,
        summary.family,
        summary.layouts,
        summary.families,
        summary.skipped_families,
        summary.total_practice_secs,
        summary
            .recent_wpm
            .map_or("-".to_string(), |w| format!("{w:.4}")),
    )
}

/// Every field of a `SavedStats` except `today_date`, which is the clock's
/// local date, with keys sorted.
fn saved_report(saved: &SavedStats) -> String {
    let mut out = format!(
        "version={} total_lessons={} last_session={} today_seconds={} legacy_minutes={:?}\n\
         unlocked: {}\nlast_lesson: {}\nhistory: {}",
        saved.version,
        saved.total_lessons,
        saved.last_session,
        saved.today_seconds_practiced,
        saved.today_minutes_practiced,
        saved.unlocked_letters.iter().collect::<String>(),
        lesson_results(saved.last_lesson.as_slice()),
        lesson_results(&saved.lesson_history),
    );
    for ch in sorted_keys(&saved.keys) {
        let k = &saved.keys[&ch];
        out.push_str(&format!(
            "\n{ch} attempts={} errors={} filtered={:.4} best={:.4} recent={:?}",
            k.attempts, k.errors, k.filtered_time_ms, k.best_filtered_time_ms, k.recent_times_ms,
        ));
    }
    out
}

fn import_report(choice: LayoutChoice) -> String {
    let records: Vec<ExportRecord> = serde_json::from_str(EXPORT_JSON).unwrap();
    let (saved, summary) = replay(&records, TARGET_CPM, Language::English, choice);
    format!(
        "choice: {choice:?}\n{}\n{}",
        summary_report(&summary),
        saved_report(&saved)
    )
}

#[test]
fn golden_import_latest_family() {
    check(
        "IMPORT_LATEST_FAMILY",
        &import_report(LayoutChoice::Latest),
        IMPORT_LATEST_FAMILY,
    );
}

#[test]
fn golden_import_all_families() {
    check(
        "IMPORT_ALL_FAMILIES",
        &import_report(LayoutChoice::parse("ALL").unwrap()),
        IMPORT_ALL_FAMILIES,
    );
}

#[test]
fn golden_import_named_family() {
    // A layout id stands for its family.
    check(
        "IMPORT_NAMED_FAMILY",
        &import_report(LayoutChoice::parse("en-colemak").unwrap()),
        IMPORT_NAMED_FAMILY,
    );
}

// ---------------------------------------------------------------------------
// 4. On-disk formats
// ---------------------------------------------------------------------------
//
// `SavedStats::save` and `Config::save` write to the user's real data and
// config dirs, so these take the bytes from the functions every save
// writes (`SavedStats::to_json` and `Config::to_toml`) instead.

fn stats_json(saved: &SavedStats) -> String {
    saved.to_json().unwrap()
}

fn config_toml(config: &Config) -> String {
    config.to_toml().unwrap()
}

#[test]
fn golden_stats_json_format() {
    let mut keys = HashMap::new();
    keys.insert(
        'e',
        SavedKeyStats {
            attempts: 6,
            errors: 1,
            filtered_time_ms: 208.0,
            best_filtered_time_ms: 207.5,
            recent_times_ms: vec![210, 190, 230],
        },
    );
    let saved = SavedStats {
        version: 2,
        keys,
        unlocked_letters: "eniarl".chars().collect(),
        total_lessons: 12,
        last_session: "1700000000".to_string(),
        today_seconds_practiced: 125,
        // Read-only legacy field: never written.
        today_minutes_practiced: Some(3),
        today_date: "2026-11-06".to_string(),
        last_lesson: Some(SavedLessonResult {
            wpm: 41.5,
            accuracy: 97.25,
        }),
        lesson_history: vec![
            SavedLessonResult {
                wpm: 38.0,
                accuracy: 94.0,
            },
            SavedLessonResult {
                wpm: 41.5,
                accuracy: 97.25,
            },
        ],
    };
    check("STATS_JSON", &stats_json(&saved), STATS_JSON);
}

#[test]
fn golden_stats_json_format_non_ascii() {
    let mut keys = HashMap::new();
    keys.insert(
        'ı',
        SavedKeyStats {
            attempts: 1,
            errors: 1,
            filtered_time_ms: 650.0,
            best_filtered_time_ms: 650.0,
            recent_times_ms: vec![650],
        },
    );
    let saved = SavedStats {
        version: 2,
        keys,
        unlocked_letters: "aeinır".chars().collect(),
        total_lessons: 0,
        last_session: String::new(),
        today_seconds_practiced: 0,
        today_minutes_practiced: None,
        today_date: String::new(),
        last_lesson: None,
        lesson_history: Vec::new(),
    };
    check(
        "STATS_JSON_NON_ASCII",
        &stats_json(&saved),
        STATS_JSON_NON_ASCII,
    );
}

#[test]
fn golden_stats_json_legacy_file_reserializes() {
    // A pre-v0.2.1 file: no version, legacy minutes, no lesson fields.
    let legacy = r#"{
        "keys": {"t": {"attempts": 3, "errors": 0, "filtered_time_ms": 300.5,
                       "best_filtered_time_ms": 290.25, "recent_times_ms": [300, 301]}},
        "unlocked_letters": ["e", "t", "a", "o", "i", "n"],
        "total_lessons": 4,
        "last_session": "1600000000",
        "today_minutes_practiced": 2,
        "today_date": "2020-09-13"
    }"#;
    let saved: SavedStats = serde_json::from_str(legacy).unwrap();
    check(
        "STATS_JSON_LEGACY",
        &format!("{}\n--\n{}", saved_report(&saved), stats_json(&saved)),
        STATS_JSON_LEGACY,
    );
}

#[test]
fn golden_app_to_saved_stats() {
    // Restore through the app and write back: recent times are capped at
    // 20, the stats-file fields round-trip. The two clock fields are
    // overwritten before formatting.
    let mut keys = HashMap::new();
    keys.insert(
        'e',
        SavedKeyStats {
            attempts: 30,
            errors: 2,
            filtered_time_ms: 250.75,
            best_filtered_time_ms: 240.5,
            recent_times_ms: (200..225).collect(),
        },
    );
    let saved = SavedStats {
        version: 2,
        keys,
        unlocked_letters: "enziarlt".chars().collect(),
        total_lessons: 9,
        last_session: "1700000000".to_string(),
        today_seconds_practiced: 600,
        today_minutes_practiced: None,
        today_date: "2001-01-01".to_string(),
        last_lesson: Some(SavedLessonResult {
            wpm: 44.0,
            accuracy: 98.5,
        }),
        lesson_history: vec![
            SavedLessonResult {
                wpm: 40.0,
                accuracy: 95.0,
            },
            SavedLessonResult {
                wpm: 44.0,
                accuracy: 98.5,
            },
        ],
    };
    let app = App::new_with_language(
        TARGET_WPM,
        ErrorMode::ForgiveMistakes,
        Some(saved),
        Language::English,
    );
    let mut written = app.to_saved_stats();
    written.last_session = "<clock>".to_string();
    written.today_date = "<clock>".to_string();
    check(
        "APP_TO_SAVED_STATS",
        &stats_json(&written),
        APP_TO_SAVED_STATS,
    );
}

#[test]
fn golden_config_toml_default() {
    check(
        "CONFIG_TOML_DEFAULT",
        &config_toml(&Config::default()),
        CONFIG_TOML_DEFAULT,
    );
}

#[test]
fn golden_config_toml_full() {
    let config = Config {
        target_wpm: 50,
        error_mode: ErrorModeSerde::StopOnError,
        fragment_length: 80,
        natural_words: false,
        daily_goal_minutes: 15,
        alphabet_size: 0.25,
        focus_letter: Some('ş'),
        focus_pattern: Some("-ing".to_string()),
        language: "tr".to_string(),
    };
    check("CONFIG_TOML_FULL", &config_toml(&config), CONFIG_TOML_FULL);
}

#[test]
fn golden_config_toml_hand_edited_reserializes() {
    // Out-of-range alphabet size is clamped, unknown keys are dropped.
    let hand_edited = "target_wpm = 42\nerror_mode = \"stop-on-error\"\n\
                       alphabet_size = 7.5\nfocus_letter = \"r\"\nlanguage = \"de\"\n\
                       some_future_key = true\n";
    let config: Config = toml::from_str(hand_edited).unwrap();
    check(
        "CONFIG_TOML_HAND_EDITED",
        &config_toml(&config),
        CONFIG_TOML_HAND_EDITED,
    );
}

#[test]
fn golden_app_to_config() {
    // What the app writes back after launching with each lesson's config.
    let drill = LessonCase {
        fragment_length: 120,
        focus_letter: Some('y'),
        focus_pattern: Some("-ing"),
        ..case(&EN_LATER, 0)
    };
    let turkish = LessonCase {
        natural_words: false,
        focus_letter: Some('I'),
        ..case(&TR_EARLY, 0)
    };
    // A German session holding a pin it cannot use but Turkish could.
    let foreign = LessonCase {
        focus_letter: Some('ş'),
        ..case(&DE_EARLY, 0)
    };
    let mut out = Vec::new();
    for lesson in [drill, turkish, foreign] {
        let app = launch(lesson.state, &lesson.config());
        out.push(config_toml(&app.to_config()));
    }
    check("APP_TO_CONFIG", &out.join("--\n"), APP_TO_CONFIG);
}

// ---------------------------------------------------------------------------
// 5. Keystrokes through `update`
// ---------------------------------------------------------------------------

/// One scripted key press on the Typing screen.
#[derive(Clone, Copy, Debug)]
enum Stroke {
    /// The target key, pressed this many ms after it became the target.
    Hit(u64),
    /// A key that is not the target.
    Miss,
    Backspace,
    /// Toggle the error mode.
    Tab,
}

/// The key every `Miss` presses. A fresh English profile is many
/// lessons away from unlocking `q`, so it is never the target.
const MISS: char = 'q';

/// How long the scripts wait before a hit. Well inside the 40..=12000 ms
/// sample window, and far below the ~343 ms a key needs to count as
/// learned, so a slow test machine cannot change what the scheduler
/// sees after the lesson.
const HIT_MS: u64 = 100;

fn press(app: &mut App, code: KeyCode) {
    update(app, AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

/// Reaction-time samples taken across every key. `record_hit` counts an
/// attempt and `record_error` counts an attempt and an error, so the
/// hits are what is left.
fn samples(app: &App) -> u32 {
    app.per_key_stats
        .values()
        .map(|s| s.attempts - s.errors)
        .sum()
}

fn sorted_positions(set: &HashSet<usize>) -> Vec<usize> {
    let mut positions: Vec<usize> = set.iter().copied().collect();
    positions.sort_unstable();
    positions
}

/// The lesson's keystroke accounting, none of it read from the clock.
fn typing_state(app: &App) -> String {
    format!(
        "cursor={} positions={} errors={} correct={} first={} recovered={:?} \
         marked={:?} ever={:?} samples={} mode={:?}",
        app.cursor_pos,
        app.lesson_positions,
        app.lesson_errors,
        app.lesson_correct,
        app.first_attempt_correct.len(),
        sorted_positions(&app.recovered_positions),
        sorted_positions(&app.error_positions),
        sorted_positions(&app.ever_error_positions),
        samples(app),
        app.error_mode,
    )
}

/// Press `stroke`. A `Hit` first moves its target's start back by its
/// ms, which is how the scripts control which hits are timed. A lesson's
/// first key starts that clock itself, so scripts open with a `Miss`,
/// whose timing never matters.
fn play(app: &mut App, stroke: Stroke) {
    let target = app.generated_text.chars().nth(app.cursor_pos);
    match stroke {
        Stroke::Hit(ms) => {
            assert!(
                app.lesson_start.is_some(),
                "a lesson's first key times itself"
            );
            app.key_target_start = Some(
                Instant::now()
                    .checked_sub(Duration::from_millis(ms))
                    .expect("the monotonic clock has run this long"),
            );
            press(app, KeyCode::Char(target.expect("a target")));
        }
        Stroke::Miss => {
            assert_ne!(target, Some(MISS), "a miss must not be the target");
            press(app, KeyCode::Char(MISS));
        }
        Stroke::Backspace => press(app, KeyCode::Backspace),
        Stroke::Tab => press(app, KeyCode::Tab),
    }
}

/// A fresh English profile in `mode` starts a lesson from the menu and
/// plays `script`, then hits every key up to the last one and presses
/// `last` on that. Reports each scripted step, then what finishing the
/// lesson left behind. Nothing reported is read from the clock: no WPM,
/// no latencies, no daily goal.
fn keystroke_report(mode: ErrorMode, seed: u32, script: &[Stroke], last: Stroke) -> String {
    let language = Language::English;
    let mut app = App::new_with_language(TARGET_WPM, mode, None, language);
    app.generator = WordGenerator::with_seed_for(seed, language);
    // Enter on the menu's first entry, Start Practice.
    press(&mut app, KeyCode::Enter);
    let mut out = vec![format!(
        "start: screen={:?} timed={} text: {}",
        app.screen,
        app.lesson_start.is_some(),
        app.generated_text,
    )];

    for (step, &stroke) in script.iter().enumerate() {
        let target = app.generated_text.chars().nth(app.cursor_pos);
        play(&mut app, stroke);
        out.push(format!(
            "{step:>2} {stroke:?} at {target:?}: timed={} {}",
            app.lesson_start.is_some(),
            typing_state(&app),
        ));
    }

    let len = app.generated_text.chars().count();
    let mut hits = 0;
    while app.cursor_pos + 1 < len {
        play(&mut app, Stroke::Hit(HIT_MS));
        hits += 1;
    }
    out.push(format!("then {hits} hits: {}", typing_state(&app)));
    let target = app.generated_text.chars().nth(app.cursor_pos);
    play(&mut app, last);
    out.push(format!("{last:?} at {target:?} ends the lesson"));

    let result = app.last_lesson.as_ref().expect("a finished lesson");
    out.push(format!(
        "after: lessons={} history={} accuracy={:.4} unlocked={:?} screen={:?} timed={} \
         save_stats={} save_config={}",
        app.lesson_count,
        app.lesson_history.len(),
        result.accuracy,
        result.newly_unlocked,
        app.screen,
        app.lesson_start.is_some(),
        app.pending_stats_save,
        app.pending_config_save,
    ));
    out.push(format!("after: {}", typing_state(&app)));
    for ch in sorted_keys(&app.per_key_stats) {
        let st = &app.per_key_stats[&ch];
        out.push(format!(
            "{ch} attempts={} errors={} recent={}",
            st.attempts,
            st.errors,
            st.reaction_times_ms.len(),
        ));
    }
    let active: String = app.scheduler.active_keys.iter().collect();
    out.push(format!(
        "active: {active} focus: {}",
        rule_str(app.effective_focus()),
    ));
    out.push(format!("next text: {}", app.generated_text));
    out.join("\n")
}

/// Seed 3 opens with "rear era earn". A miss moves on; a fixed miss is
/// recovered and untimed; a hit after 20 s is not even an attempt;
/// retyping a letter after Backspace times it again; a missed space
/// touches no key; a second miss on one position counts once for the
/// lesson; the last key, missed, still ends the lesson.
#[test]
fn golden_keystrokes_forgive_mistakes() {
    use Stroke::*;
    let script = [
        Miss,
        Backspace,
        Hit(HIT_MS),
        Hit(HIT_MS),
        Hit(20_000),
        Hit(HIT_MS),
        Backspace,
        Hit(HIT_MS),
        Miss,
        Miss,
        Tab,
        Miss,
        Tab,
        Miss,
    ];
    check(
        "KEYSTROKES_FORGIVE_MISTAKES",
        &keystroke_report(ErrorMode::ForgiveMistakes, 3, &script, Miss),
        KEYSTROKES_FORGIVE_MISTAKES,
    );
}

/// The same lesson in stop-on-error. A miss holds the cursor and its
/// mark outlives the fix; a repeated miss counts once for the lesson;
/// Backspace clears only the position it lands on; Tab switches modes
/// mid-lesson; a missed space touches no key.
#[test]
fn golden_keystrokes_stop_on_error() {
    use Stroke::*;
    let script = [
        Miss,
        Miss,
        Hit(HIT_MS),
        Hit(HIT_MS),
        Miss,
        Backspace,
        Hit(HIT_MS),
        Hit(HIT_MS),
        Tab,
        Miss,
        Tab,
        Miss,
        Hit(HIT_MS),
    ];
    check(
        "KEYSTROKES_STOP_ON_ERROR",
        &keystroke_report(ErrorMode::StopOnError, 3, &script, Hit(HIT_MS)),
        KEYSTROKES_STOP_ON_ERROR,
    );
}

// ---------------------------------------------------------------------------
// Pinned values (captured from v0.3.1)
// ---------------------------------------------------------------------------

const LESSON_EN_EARLY_PHONETIC: &[&str] = &[
    "language: en",
    "active: eniarl (ENIARL)",
    "auto focus: l",
    "focus: l label=L pinned=false",
    "drills: []",
    "chars: 104",
    "text: lean lear line lare lain all len lan lear lie lier alreal line leare larline learlear ill learlin relain",
];
const LESSON_EN_EARLY_NATURAL: &[&str] = &[
    "language: en",
    "active: eniarl (ENIARL)",
    "auto focus: l",
    "focus: l label=L pinned=false",
    "drills: []",
    "chars: 104",
    "text: nail rail ill linen lean learn liar lie lair earlier alien line all line real ill lane ill rail ill rail",
];
const LESSON_EN_UNLOCK_NATURAL: &[&str] = &[
    "language: en",
    "active: eniarlt (ENIARLT)",
    "auto focus: t",
    "focus: t label=T pinned=false",
    "drills: [-er]",
    "chars: 106",
    "text: eternal tell intent later rat eaten lantern tall net tilt litter tie tree rental trial entire train attire",
];
const LESSON_EN_LATER_NATURAL: &[&str] = &[
    "language: en",
    "active: eniarltosudycghp (ENIARLTOSUDYCGHP)",
    "auto focus: h",
    "focus: h label=H pinned=false",
    "drills: [cr,pl,tr,gr,cl,-ing,-ed,-er,-ly,-tion,-ous]",
    "chars: 104",
    "text: theater gash punch threads hull ushered soothed photo attached theories launch arching theory hope hoped",
];
const LESSON_EN_LATER_PHONETIC: &[&str] = &[
    "language: en",
    "active: eniarltosudycghp (ENIARLTOSUDYCGHP)",
    "auto focus: h",
    "focus: h label=H pinned=false",
    "drills: [cr,pl,tr,gr,cl,-ing,-ed,-er,-ly,-tion,-ous]",
    "chars: 102",
    "text: hird lah hould had her thud thhold that happrope his head hool hadore ahead thhollap the hight helping",
];
const LESSON_EN_MANUAL_FOCUS_LETTER: &[&str] = &[
    "language: en",
    "active: eniarltosudycghp (ENIARLTOSUDYCGHP)",
    "auto focus: h",
    "focus: y label=Y pinned=true",
    "drills: [cr,pl,tr,gr,cl,-ing,-ed,-er,-ly,-tion,-ous]",
    "chars: 67",
    "text: yard openly surgery annoying rarely cycle uncertainty handy typical",
];
const LESSON_EN_DRILL_SUFFIX: &[&str] = &[
    "language: en",
    "active: eniarltosudycghp (ENIARLTOSUDYCGHP)",
    "auto focus: h",
    "focus: -ing label=-ING pinned=true",
    "drills: [cr,pl,tr,gr,cl,-ing,-ed,-er,-ly,-tion,-ous]",
    "chars: 106",
    "text: lying curling cleaning snarling leaning helping pacing agreeing teaching reeling dragging signing treating",
];
const LESSON_EN_DRILL_BIGRAM_PHONETIC: &[&str] = &[
    "language: en",
    "active: eniarltosudycghp (ENIARLTOSUDYCGHP)",
    "auto focus: h",
    "focus: tr label=TR pinned=true",
    "drills: [cr,pl,tr,gr,cl,-ing,-ed,-er,-ly,-tion,-ous]",
    "chars: 103",
    "text: traces strangled triple entrance transition traditional electrical strength penetrating nostrils strips",
];
const LESSON_DE_EARLY_PHONETIC: &[&str] = &[
    "language: de",
    "active: entirl (ENTIRL)",
    "auto focus: l",
    "focus: l label=L pinned=false",
    "drills: []",
    "chars: 103",
    "text: lein lie lette lett eilett linie leine erlie lern lier lien letter etliter ler lin illin lei lein elter",
];
const LESSON_DE_LATER_NATURAL: &[&str] = &[
    "language: de",
    "active: entirlhascgumofdkbpzüä (ENTIRLHASCGUMOFDKBPZÜÄ)",
    "auto focus: ä",
    "focus: ä label=Ä pinned=false",
    "drills: []",
    "chars: 107",
    "text: enthält schläft säuberlich hältst zärtlichen bekäme unablässig nachträglich alltäglich bedächtigen glänzend",
];
const LESSON_TR_EARLY_PINNED_DOTLESS_I: &[&str] = &[
    "language: tr",
    "active: aeinır (AEİNIR)",
    "auto focus: r",
    "focus: ı label=I pinned=true",
    "drills: []",
    "chars: 102",
    "text: anın arı anırı nın anı anırı inanın nın anı inanır arı arı anırı nın inanır arı anı arı arı anı inanır",
];
const LESSON_TR_LATER_NATURAL: &[&str] = &[
    "language: tr",
    "active: aeinırlutmküoy (AEİNIRLUTMKÜOY)",
    "auto focus: ü",
    "focus: ü label=Ü pinned=false",
    "drills: []",
    "chars: 107",
    "text: kontrolü yürüyelim yürüyerek kül ünlü yüklü ülkeyi mükemmel tüy yürümek türlü tüm tünel kürek tüylü nükleer",
];
const LESSON_RU_EARLY_NATURAL: &[&str] = &[
    "language: ru",
    "active: атиоен (АТИОЕН)",
    "auto focus: е",
    "focus: е label=Е pinned=false",
    "drills: []",
    "chars: 101",
    "text: теат отена атент антенна нет атено нее нее нет инение тет тет атент нет нет антенна нее нет атент нее",
];
const SCHEDULER_EN_EARLY: &[&str] = &[
    "language: en restored=true alphabet_size=0.00 forced_extra=0",
    "active: eniarl",
    "focus: l",
    "a attempts=3 errors=0 filtered=299.0000 best=299.0000 conf=1.146679 best_conf=1.146679 wpm=40.134 best_wpm=40.134 recent=[300, 280, 300]",
    "e attempts=6 errors=1 filtered=208.0000 best=208.0000 conf=1.648352 best_conf=1.648352 wpm=57.692 best_wpm=57.692 recent=[210, 190, 230, 200, 180]",
    "i attempts=5 errors=2 filtered=367.0000 best=367.0000 conf=0.934216 best_conf=0.934216 wpm=32.698 best_wpm=32.698 recent=[360, 380, 340]",
    "n attempts=3 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[260, 240, 250]",
    "o attempts=4 errors=0 filtered=385.7500 best=250.0000 conf=0.888807 best_conf=1.371429 wpm=31.108 best_wpm=48.000 recent=[250, 900, 950, 1000]",
    "r attempts=7 errors=3 filtered=538.0000 best=538.0000 conf=0.637281 best_conf=0.637281 wpm=22.305 best_wpm=22.305 recent=[520, 560, 540, 500]",
];
const SCHEDULER_EN_EARLY_ALPHABET_SIZE: &[&str] = &[
    "language: en restored=true alphabet_size=0.50 forced_extra=10",
    "active: eniarltosudycghp",
    "focus: l",
    "a attempts=3 errors=0 filtered=299.0000 best=299.0000 conf=1.146679 best_conf=1.146679 wpm=40.134 best_wpm=40.134 recent=[300, 280, 300]",
    "e attempts=6 errors=1 filtered=208.0000 best=208.0000 conf=1.648352 best_conf=1.648352 wpm=57.692 best_wpm=57.692 recent=[210, 190, 230, 200, 180]",
    "i attempts=5 errors=2 filtered=367.0000 best=367.0000 conf=0.934216 best_conf=0.934216 wpm=32.698 best_wpm=32.698 recent=[360, 380, 340]",
    "n attempts=3 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[260, 240, 250]",
    "o attempts=4 errors=0 filtered=385.7500 best=250.0000 conf=0.888807 best_conf=1.371429 wpm=31.108 best_wpm=48.000 recent=[250, 900, 950, 1000]",
    "r attempts=7 errors=3 filtered=538.0000 best=538.0000 conf=0.637281 best_conf=0.637281 wpm=22.305 best_wpm=22.305 recent=[520, 560, 540, 500]",
];
const SCHEDULER_EN_UNLOCK: &[&str] = &[
    "language: en restored=true alphabet_size=0.00 forced_extra=0",
    "active: eniarlt",
    "focus: t",
    "a attempts=3 errors=0 filtered=299.0000 best=299.0000 conf=1.146679 best_conf=1.146679 wpm=40.134 best_wpm=40.134 recent=[300, 280, 300]",
    "e attempts=6 errors=1 filtered=208.0000 best=208.0000 conf=1.648352 best_conf=1.648352 wpm=57.692 best_wpm=57.692 recent=[210, 190, 230, 200, 180]",
    "i attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "l attempts=1 errors=0 filtered=330.0000 best=330.0000 conf=1.038961 best_conf=1.038961 wpm=36.364 best_wpm=36.364 recent=[330]",
    "n attempts=3 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[260, 240, 250]",
    "r attempts=1 errors=0 filtered=320.0000 best=320.0000 conf=1.071429 best_conf=1.071429 wpm=37.500 best_wpm=37.500 recent=[320]",
];
const SCHEDULER_EN_LATER: &[&str] = &[
    "language: en restored=true alphabet_size=0.00 forced_extra=0",
    "active: eniarltosudycghp",
    "focus: h",
    "a attempts=3 errors=0 filtered=299.0000 best=299.0000 conf=1.146679 best_conf=1.146679 wpm=40.134 best_wpm=40.134 recent=[300, 280, 300]",
    "c attempts=1 errors=0 filtered=330.0000 best=330.0000 conf=1.038961 best_conf=1.038961 wpm=36.364 best_wpm=36.364 recent=[330]",
    "d attempts=1 errors=0 filtered=290.0000 best=290.0000 conf=1.182266 best_conf=1.182266 wpm=41.379 best_wpm=41.379 recent=[290]",
    "e attempts=6 errors=1 filtered=208.0000 best=208.0000 conf=1.648352 best_conf=1.648352 wpm=57.692 best_wpm=57.692 recent=[210, 190, 230, 200, 180]",
    "g attempts=1 errors=0 filtered=450.0000 best=450.0000 conf=0.761905 best_conf=0.761905 wpm=26.667 best_wpm=26.667 recent=[450]",
    "h attempts=6 errors=4 filtered=610.0000 best=610.0000 conf=0.562061 best_conf=0.562061 wpm=19.672 best_wpm=19.672 recent=[600, 620]",
    "i attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "l attempts=1 errors=0 filtered=330.0000 best=330.0000 conf=1.038961 best_conf=1.038961 wpm=36.364 best_wpm=36.364 recent=[330]",
    "m attempts=1 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[250]",
    "n attempts=3 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[260, 240, 250]",
    "o attempts=4 errors=0 filtered=385.7500 best=250.0000 conf=0.888807 best_conf=1.371429 wpm=31.108 best_wpm=48.000 recent=[250, 900, 950, 1000]",
    "p attempts=1 errors=0 filtered=380.0000 best=380.0000 conf=0.902256 best_conf=0.902256 wpm=31.579 best_wpm=31.579 recent=[380]",
    "r attempts=1 errors=0 filtered=320.0000 best=320.0000 conf=1.071429 best_conf=1.071429 wpm=37.500 best_wpm=37.500 recent=[320]",
    "s attempts=1 errors=0 filtered=310.0000 best=310.0000 conf=1.105991 best_conf=1.105991 wpm=38.710 best_wpm=38.710 recent=[310]",
    "t attempts=2 errors=0 filtered=290.0000 best=290.0000 conf=1.182266 best_conf=1.182266 wpm=41.379 best_wpm=41.379 recent=[280, 300]",
    "u attempts=1 errors=0 filtered=335.0000 best=335.0000 conf=1.023454 best_conf=1.023454 wpm=35.821 best_wpm=35.821 recent=[335]",
    "y attempts=3 errors=1 filtered=410.0000 best=410.0000 conf=0.836237 best_conf=0.836237 wpm=29.268 best_wpm=29.268 recent=[400, 420]",
];
const SCHEDULER_DE_LATER: &[&str] = &[
    "language: de restored=true alphabet_size=0.00 forced_extra=0",
    "active: entirlhascgumofdkbpzüä",
    "focus: ä",
    "a attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "b attempts=1 errors=0 filtered=285.0000 best=285.0000 conf=1.203008 best_conf=1.203008 wpm=42.105 best_wpm=42.105 recent=[285]",
    "c attempts=1 errors=0 filtered=320.0000 best=320.0000 conf=1.071429 best_conf=1.071429 wpm=37.500 best_wpm=37.500 recent=[320]",
    "d attempts=1 errors=0 filtered=265.0000 best=265.0000 conf=1.293801 best_conf=1.293801 wpm=45.283 best_wpm=45.283 recent=[265]",
    "e attempts=1 errors=0 filtered=230.0000 best=230.0000 conf=1.490683 best_conf=1.490683 wpm=52.174 best_wpm=52.174 recent=[230]",
    "f attempts=1 errors=0 filtered=255.0000 best=255.0000 conf=1.344538 best_conf=1.344538 wpm=47.059 best_wpm=47.059 recent=[255]",
    "g attempts=1 errors=0 filtered=330.0000 best=330.0000 conf=1.038961 best_conf=1.038961 wpm=36.364 best_wpm=36.364 recent=[330]",
    "h attempts=1 errors=0 filtered=290.0000 best=290.0000 conf=1.182266 best_conf=1.182266 wpm=41.379 best_wpm=41.379 recent=[290]",
    "i attempts=1 errors=0 filtered=260.0000 best=260.0000 conf=1.318681 best_conf=1.318681 wpm=46.154 best_wpm=46.154 recent=[260]",
    "k attempts=1 errors=0 filtered=275.0000 best=275.0000 conf=1.246753 best_conf=1.246753 wpm=43.636 best_wpm=43.636 recent=[275]",
    "l attempts=1 errors=0 filtered=280.0000 best=280.0000 conf=1.224490 best_conf=1.224490 wpm=42.857 best_wpm=42.857 recent=[280]",
    "m attempts=1 errors=0 filtered=225.0000 best=225.0000 conf=1.523810 best_conf=1.523810 wpm=53.333 best_wpm=53.333 recent=[225]",
    "n attempts=1 errors=0 filtered=240.0000 best=240.0000 conf=1.428571 best_conf=1.428571 wpm=50.000 best_wpm=50.000 recent=[240]",
    "o attempts=1 errors=0 filtered=245.0000 best=245.0000 conf=1.399417 best_conf=1.399417 wpm=48.980 best_wpm=48.980 recent=[245]",
    "p attempts=1 errors=0 filtered=295.0000 best=295.0000 conf=1.162228 best_conf=1.162228 wpm=40.678 best_wpm=40.678 recent=[295]",
    "r attempts=1 errors=0 filtered=270.0000 best=270.0000 conf=1.269841 best_conf=1.269841 wpm=44.444 best_wpm=44.444 recent=[270]",
    "s attempts=1 errors=0 filtered=310.0000 best=310.0000 conf=1.105991 best_conf=1.105991 wpm=38.710 best_wpm=38.710 recent=[310]",
    "t attempts=1 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[250]",
    "u attempts=1 errors=0 filtered=335.0000 best=335.0000 conf=1.023454 best_conf=1.023454 wpm=35.821 best_wpm=35.821 recent=[335]",
    "z attempts=1 errors=0 filtered=305.0000 best=305.0000 conf=1.124122 best_conf=1.124122 wpm=39.344 best_wpm=39.344 recent=[305]",
    "ß attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "ü attempts=4 errors=2 filtered=500.0000 best=500.0000 conf=0.685714 best_conf=0.685714 wpm=24.000 best_wpm=24.000 recent=[520, 480]",
];
const SCHEDULER_TR_EARLY: &[&str] = &[
    "language: tr restored=true alphabet_size=0.00 forced_extra=0",
    "active: aeinır",
    "focus: r",
    "a attempts=1 errors=0 filtered=240.0000 best=240.0000 conf=1.428571 best_conf=1.428571 wpm=50.000 best_wpm=50.000 recent=[240]",
    "e attempts=1 errors=0 filtered=260.0000 best=260.0000 conf=1.318681 best_conf=1.318681 wpm=46.154 best_wpm=46.154 recent=[260]",
    "i attempts=1 errors=0 filtered=420.0000 best=420.0000 conf=0.816327 best_conf=0.816327 wpm=28.571 best_wpm=28.571 recent=[420]",
    "n attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "ı attempts=2 errors=1 filtered=650.0000 best=650.0000 conf=0.527473 best_conf=0.527473 wpm=18.462 best_wpm=18.462 recent=[650]",
];
const SCHEDULER_RU_EARLY: &[&str] = &[
    "language: ru restored=true alphabet_size=0.00 forced_extra=0",
    "active: атиоен",
    "focus: е",
    "а attempts=1 errors=0 filtered=250.0000 best=250.0000 conf=1.371429 best_conf=1.371429 wpm=48.000 best_wpm=48.000 recent=[250]",
    "и attempts=4 errors=2 filtered=490.0000 best=490.0000 conf=0.699708 best_conf=0.699708 wpm=24.490 best_wpm=24.490 recent=[480, 500]",
    "н attempts=1 errors=0 filtered=330.0000 best=330.0000 conf=1.038961 best_conf=1.038961 wpm=36.364 best_wpm=36.364 recent=[330]",
    "о attempts=1 errors=0 filtered=300.0000 best=300.0000 conf=1.142857 best_conf=1.142857 wpm=40.000 best_wpm=40.000 recent=[300]",
    "т attempts=1 errors=0 filtered=280.0000 best=280.0000 conf=1.224490 best_conf=1.224490 wpm=42.857 best_wpm=42.857 recent=[280]",
];
const IMPORT_LATEST_FAMILY: &[&str] = &[
    "choice: Latest",
    "total=6 imported=3 skipped_layout=1 skipped_family=1 skipped_invalid=1",
    "unlocked_letters=7 focused_key=Some('t') family=Some(\"qwerty\")",
    "layouts=[\"en-us\", \"en-uk\"] families=[\"qwerty\"] skipped_families=[\"colemak\"]",
    "total_practice_secs=135 recent_wpm=41.3333",
    "version=2 total_lessons=3 last_session=2021-03-05T20:00:00.000Z today_seconds=0 legacy_minutes=None",
    "unlocked: eniarlt",
    "last_lesson: 48.0000/0.0000",
    "history: 36.0000/95.0000 40.0000/98.0000 48.0000/0.0000",
    "a attempts=56 errors=0 filtered=309.2000 best=309.2000 recent=[320, 300, 230]",
    "e attempts=105 errors=1 filtered=243.2000 best=243.2000 recent=[250, 230, 200]",
    "i attempts=49 errors=3 filtered=291.1000 best=291.1000 recent=[300, 290, 220]",
    "l attempts=25 errors=1 filtered=340.0000 best=340.0000 recent=[350, 250]",
    "n attempts=63 errors=1 filtered=271.2000 best=271.2000 recent=[280, 260, 210]",
    "o attempts=9 errors=0 filtered=270.0000 best=270.0000 recent=[270]",
    "r attempts=37 errors=3 filtered=301.2000 best=300.0000 recent=[300, 380, 240]",
    "t attempts=16 errors=1 filtered=476.0000 best=476.0000 recent=[500, 260]",
];
const IMPORT_ALL_FAMILIES: &[&str] = &[
    "choice: All",
    "total=6 imported=4 skipped_layout=1 skipped_family=0 skipped_invalid=1",
    "unlocked_letters=7 focused_key=Some('t') family=None",
    "layouts=[\"en-colemak\", \"en-us\", \"en-uk\"] families=[\"colemak\", \"qwerty\"] skipped_families=[]",
    "total_practice_secs=165 recent_wpm=41.0000",
    "version=2 total_lessons=4 last_session=2021-03-05T20:00:00.000Z today_seconds=0 legacy_minutes=None",
    "unlocked: eniarlt",
    "last_lesson: 48.0000/0.0000",
    "history: 40.0000/96.0000 36.0000/95.0000 40.0000/98.0000 48.0000/0.0000",
    "a attempts=66 errors=0 filtered=229.0100 best=210.0000 recent=[210, 320, 300, 230]",
    "e attempts=125 errors=2 filtered=192.1700 best=180.0000 recent=[180, 250, 230, 200]",
    "i attempts=61 errors=5 filtered=218.2000 best=200.0000 recent=[200, 300, 290, 220]",
    "l attempts=25 errors=1 filtered=340.0000 best=340.0000 recent=[350, 250]",
    "n attempts=78 errors=1 filtered=205.5900 best=190.0000 recent=[190, 280, 260, 210]",
    "o attempts=9 errors=0 filtered=270.0000 best=270.0000 recent=[270]",
    "r attempts=37 errors=3 filtered=301.2000 best=300.0000 recent=[300, 380, 240]",
    "t attempts=16 errors=1 filtered=476.0000 best=476.0000 recent=[500, 260]",
];
const IMPORT_NAMED_FAMILY: &[&str] = &[
    "choice: Family(\"colemak\")",
    "total=6 imported=1 skipped_layout=1 skipped_family=4 skipped_invalid=0",
    "unlocked_letters=6 focused_key=Some('r') family=Some(\"colemak\")",
    "layouts=[\"en-colemak\"] families=[\"colemak\"] skipped_families=[\"qwerty\"]",
    "total_practice_secs=30 recent_wpm=40.0000",
    "version=2 total_lessons=1 last_session=2021-02-01T08:00:00.000Z today_seconds=0 legacy_minutes=None",
    "unlocked: eniarl",
    "last_lesson: 40.0000/96.0000",
    "history: 40.0000/96.0000",
    "a attempts=10 errors=0 filtered=210.0000 best=210.0000 recent=[210]",
    "e attempts=20 errors=1 filtered=180.0000 best=180.0000 recent=[180]",
    "i attempts=12 errors=2 filtered=200.0000 best=200.0000 recent=[200]",
    "n attempts=15 errors=0 filtered=190.0000 best=190.0000 recent=[190]",
];
const STATS_JSON: &[&str] = &[
    "{",
    "  \"version\": 2,",
    "  \"keys\": {",
    "    \"e\": {",
    "      \"attempts\": 6,",
    "      \"errors\": 1,",
    "      \"filtered_time_ms\": 208.0,",
    "      \"best_filtered_time_ms\": 207.5,",
    "      \"recent_times_ms\": [",
    "        210,",
    "        190,",
    "        230",
    "      ]",
    "    }",
    "  },",
    "  \"unlocked_letters\": [",
    "    \"e\",",
    "    \"n\",",
    "    \"i\",",
    "    \"a\",",
    "    \"r\",",
    "    \"l\"",
    "  ],",
    "  \"total_lessons\": 12,",
    "  \"last_session\": \"1700000000\",",
    "  \"today_seconds_practiced\": 125,",
    "  \"today_date\": \"2026-11-06\",",
    "  \"last_lesson\": {",
    "    \"wpm\": 41.5,",
    "    \"accuracy\": 97.25",
    "  },",
    "  \"lesson_history\": [",
    "    {",
    "      \"wpm\": 38.0,",
    "      \"accuracy\": 94.0",
    "    },",
    "    {",
    "      \"wpm\": 41.5,",
    "      \"accuracy\": 97.25",
    "    }",
    "  ]",
    "}",
];
const STATS_JSON_NON_ASCII: &[&str] = &[
    "{",
    "  \"version\": 2,",
    "  \"keys\": {",
    "    \"ı\": {",
    "      \"attempts\": 1,",
    "      \"errors\": 1,",
    "      \"filtered_time_ms\": 650.0,",
    "      \"best_filtered_time_ms\": 650.0,",
    "      \"recent_times_ms\": [",
    "        650",
    "      ]",
    "    }",
    "  },",
    "  \"unlocked_letters\": [",
    "    \"a\",",
    "    \"e\",",
    "    \"i\",",
    "    \"n\",",
    "    \"ı\",",
    "    \"r\"",
    "  ],",
    "  \"total_lessons\": 0,",
    "  \"last_session\": \"\",",
    "  \"today_seconds_practiced\": 0,",
    "  \"today_date\": \"\",",
    "  \"last_lesson\": null,",
    "  \"lesson_history\": []",
    "}",
];
const STATS_JSON_LEGACY: &[&str] = &[
    "version=1 total_lessons=4 last_session=1600000000 today_seconds=0 legacy_minutes=Some(2)",
    "unlocked: etaoin",
    "last_lesson: ",
    "history: ",
    "t attempts=3 errors=0 filtered=300.5000 best=290.2500 recent=[300, 301]",
    "--",
    "{",
    "  \"version\": 1,",
    "  \"keys\": {",
    "    \"t\": {",
    "      \"attempts\": 3,",
    "      \"errors\": 0,",
    "      \"filtered_time_ms\": 300.5,",
    "      \"best_filtered_time_ms\": 290.25,",
    "      \"recent_times_ms\": [",
    "        300,",
    "        301",
    "      ]",
    "    }",
    "  },",
    "  \"unlocked_letters\": [",
    "    \"e\",",
    "    \"t\",",
    "    \"a\",",
    "    \"o\",",
    "    \"i\",",
    "    \"n\"",
    "  ],",
    "  \"total_lessons\": 4,",
    "  \"last_session\": \"1600000000\",",
    "  \"today_seconds_practiced\": 0,",
    "  \"today_date\": \"2020-09-13\",",
    "  \"last_lesson\": null,",
    "  \"lesson_history\": []",
    "}",
];
const APP_TO_SAVED_STATS: &[&str] = &[
    "{",
    "  \"version\": 2,",
    "  \"keys\": {",
    "    \"e\": {",
    "      \"attempts\": 30,",
    "      \"errors\": 2,",
    "      \"filtered_time_ms\": 250.75,",
    "      \"best_filtered_time_ms\": 240.5,",
    "      \"recent_times_ms\": [",
    "        205,",
    "        206,",
    "        207,",
    "        208,",
    "        209,",
    "        210,",
    "        211,",
    "        212,",
    "        213,",
    "        214,",
    "        215,",
    "        216,",
    "        217,",
    "        218,",
    "        219,",
    "        220,",
    "        221,",
    "        222,",
    "        223,",
    "        224",
    "      ]",
    "    }",
    "  },",
    "  \"unlocked_letters\": [",
    "    \"e\",",
    "    \"n\",",
    "    \"z\",",
    "    \"i\",",
    "    \"a\",",
    "    \"r\",",
    "    \"l\",",
    "    \"t\"",
    "  ],",
    "  \"total_lessons\": 9,",
    "  \"last_session\": \"<clock>\",",
    "  \"today_seconds_practiced\": 0,",
    "  \"today_date\": \"<clock>\",",
    "  \"last_lesson\": {",
    "    \"wpm\": 44.0,",
    "    \"accuracy\": 98.5",
    "  },",
    "  \"lesson_history\": [",
    "    {",
    "      \"wpm\": 40.0,",
    "      \"accuracy\": 95.0",
    "    },",
    "    {",
    "      \"wpm\": 44.0,",
    "      \"accuracy\": 98.5",
    "    }",
    "  ]",
    "}",
];
const CONFIG_TOML_DEFAULT: &[&str] = &[
    "target_wpm = 35",
    "error_mode = \"forgive-mistakes\"",
    "fragment_length = 100",
    "natural_words = true",
    "daily_goal_minutes = 30",
    "alphabet_size = 0.0",
    "",
];
const CONFIG_TOML_FULL: &[&str] = &[
    "target_wpm = 50",
    "error_mode = \"stop-on-error\"",
    "fragment_length = 80",
    "natural_words = false",
    "daily_goal_minutes = 15",
    "alphabet_size = 0.25",
    "focus_letter = \"ş\"",
    "focus_pattern = \"-ing\"",
    "language = \"tr\"",
    "",
];
const CONFIG_TOML_HAND_EDITED: &[&str] = &[
    "target_wpm = 42",
    "error_mode = \"stop-on-error\"",
    "fragment_length = 100",
    "natural_words = true",
    "daily_goal_minutes = 30",
    "alphabet_size = 1.0",
    "focus_letter = \"r\"",
    "language = \"de\"",
    "",
];
const APP_TO_CONFIG: &[&str] = &[
    "target_wpm = 35",
    "error_mode = \"forgive-mistakes\"",
    "fragment_length = 120",
    "natural_words = true",
    "daily_goal_minutes = 30",
    "alphabet_size = 0.0",
    "focus_pattern = \"-ing\"",
    "--",
    "target_wpm = 35",
    "error_mode = \"forgive-mistakes\"",
    "fragment_length = 100",
    "natural_words = false",
    "daily_goal_minutes = 30",
    "alphabet_size = 0.0",
    "focus_letter = \"ı\"",
    "language = \"tr\"",
    "--",
    "target_wpm = 35",
    "error_mode = \"forgive-mistakes\"",
    "fragment_length = 100",
    "natural_words = true",
    "daily_goal_minutes = 30",
    "alphabet_size = 0.0",
    "focus_letter = \"ş\"",
    "language = \"de\"",
    "",
];
const KEYSTROKES_FORGIVE_MISTAKES: &[&str] = &[
    "start: screen=Typing timed=false text: rear era earn area arena linen nine rare lie lean lane near learn linen real learn ear line nearer inner",
    " 0 Miss at Some('r'): timed=true cursor=1 positions=1 errors=1 correct=0 first=0 recovered=[] marked=[0] ever=[0] samples=0 mode=ForgiveMistakes",
    " 1 Backspace at Some('e'): timed=true cursor=0 positions=1 errors=1 correct=0 first=0 recovered=[] marked=[] ever=[0] samples=0 mode=ForgiveMistakes",
    " 2 Hit(100) at Some('r'): timed=true cursor=1 positions=2 errors=1 correct=1 first=0 recovered=[0] marked=[] ever=[0] samples=0 mode=ForgiveMistakes",
    " 3 Hit(100) at Some('e'): timed=true cursor=2 positions=3 errors=1 correct=2 first=1 recovered=[0] marked=[] ever=[0] samples=1 mode=ForgiveMistakes",
    " 4 Hit(20000) at Some('a'): timed=true cursor=3 positions=4 errors=1 correct=3 first=2 recovered=[0] marked=[] ever=[0] samples=1 mode=ForgiveMistakes",
    " 5 Hit(100) at Some('r'): timed=true cursor=4 positions=5 errors=1 correct=4 first=3 recovered=[0] marked=[] ever=[0] samples=2 mode=ForgiveMistakes",
    " 6 Backspace at Some(' '): timed=true cursor=3 positions=5 errors=1 correct=4 first=2 recovered=[0] marked=[] ever=[0] samples=2 mode=ForgiveMistakes",
    " 7 Hit(100) at Some('r'): timed=true cursor=4 positions=6 errors=1 correct=5 first=3 recovered=[0] marked=[] ever=[0] samples=3 mode=ForgiveMistakes",
    " 8 Miss at Some(' '): timed=true cursor=5 positions=6 errors=1 correct=5 first=3 recovered=[0] marked=[4] ever=[0, 4] samples=3 mode=ForgiveMistakes",
    " 9 Miss at Some('e'): timed=true cursor=6 positions=7 errors=2 correct=5 first=3 recovered=[0] marked=[4, 5] ever=[0, 4, 5] samples=3 mode=ForgiveMistakes",
    "10 Tab at Some('r'): timed=true cursor=6 positions=7 errors=2 correct=5 first=3 recovered=[0] marked=[4, 5] ever=[0, 4, 5] samples=3 mode=StopOnError",
    "11 Miss at Some('r'): timed=true cursor=6 positions=8 errors=3 correct=5 first=3 recovered=[0] marked=[4, 5, 6] ever=[0, 4, 5, 6] samples=3 mode=StopOnError",
    "12 Tab at Some('r'): timed=true cursor=6 positions=8 errors=3 correct=5 first=3 recovered=[0] marked=[4, 5, 6] ever=[0, 4, 5, 6] samples=3 mode=ForgiveMistakes",
    "13 Miss at Some('r'): timed=true cursor=7 positions=8 errors=3 correct=5 first=3 recovered=[0] marked=[4, 5, 6] ever=[0, 4, 5, 6] samples=3 mode=ForgiveMistakes",
    "then 96 hits: cursor=103 positions=86 errors=3 correct=101 first=99 recovered=[0] marked=[4, 5, 6] ever=[0, 4, 5, 6] samples=81 mode=ForgiveMistakes",
    "Miss at Some('r') ends the lesson",
    "after: lessons=1 history=1 accuracy=95.4023 unlocked=Some('t') screen=Typing timed=false save_stats=true save_config=true",
    "after: cursor=0 positions=0 errors=0 correct=0 first=0 recovered=[] marked=[] ever=[] samples=81 mode=ForgiveMistakes",
    "a attempts=15 errors=0 recent=15",
    "e attempts=21 errors=1 recent=20",
    "i attempts=6 errors=0 recent=6",
    "l attempts=9 errors=0 recent=9",
    "n attempts=17 errors=0 recent=17",
    "r attempts=18 errors=4 recent=14",
    "active: eniarlt focus: t",
    "next text: tail intent rental taller tire alter anti trailer lantern rat tree tear later letter entire enter internet",
];
const KEYSTROKES_STOP_ON_ERROR: &[&str] = &[
    "start: screen=Typing timed=false text: rear era earn area arena linen nine rare lie lean lane near learn linen real learn ear line nearer inner",
    " 0 Miss at Some('r'): timed=true cursor=0 positions=1 errors=1 correct=0 first=0 recovered=[] marked=[0] ever=[0] samples=0 mode=StopOnError",
    " 1 Miss at Some('r'): timed=true cursor=0 positions=1 errors=1 correct=0 first=0 recovered=[] marked=[0] ever=[0] samples=0 mode=StopOnError",
    " 2 Hit(100) at Some('r'): timed=true cursor=1 positions=2 errors=1 correct=1 first=0 recovered=[0] marked=[0] ever=[0] samples=0 mode=StopOnError",
    " 3 Hit(100) at Some('e'): timed=true cursor=2 positions=3 errors=1 correct=2 first=1 recovered=[0] marked=[0] ever=[0] samples=1 mode=StopOnError",
    " 4 Miss at Some('a'): timed=true cursor=2 positions=4 errors=2 correct=2 first=1 recovered=[0] marked=[0, 2] ever=[0, 2] samples=1 mode=StopOnError",
    " 5 Backspace at Some('a'): timed=true cursor=1 positions=4 errors=2 correct=2 first=0 recovered=[0] marked=[0, 2] ever=[0, 2] samples=1 mode=StopOnError",
    " 6 Hit(100) at Some('e'): timed=true cursor=2 positions=5 errors=2 correct=3 first=1 recovered=[0] marked=[0, 2] ever=[0, 2] samples=2 mode=StopOnError",
    " 7 Hit(100) at Some('a'): timed=true cursor=3 positions=6 errors=2 correct=4 first=1 recovered=[0, 2] marked=[0, 2] ever=[0, 2] samples=2 mode=StopOnError",
    " 8 Tab at Some('r'): timed=true cursor=3 positions=6 errors=2 correct=4 first=1 recovered=[0, 2] marked=[0, 2] ever=[0, 2] samples=2 mode=ForgiveMistakes",
    " 9 Miss at Some('r'): timed=true cursor=4 positions=7 errors=3 correct=4 first=1 recovered=[0, 2] marked=[0, 2, 3] ever=[0, 2, 3] samples=2 mode=ForgiveMistakes",
    "10 Tab at Some(' '): timed=true cursor=4 positions=7 errors=3 correct=4 first=1 recovered=[0, 2] marked=[0, 2, 3] ever=[0, 2, 3] samples=2 mode=StopOnError",
    "11 Miss at Some(' '): timed=true cursor=4 positions=7 errors=3 correct=4 first=1 recovered=[0, 2] marked=[0, 2, 3, 4] ever=[0, 2, 3, 4] samples=2 mode=StopOnError",
    "12 Hit(100) at Some(' '): timed=true cursor=5 positions=7 errors=3 correct=5 first=1 recovered=[0, 2, 4] marked=[0, 2, 3, 4] ever=[0, 2, 3, 4] samples=2 mode=StopOnError",
    "then 98 hits: cursor=103 positions=87 errors=3 correct=103 first=99 recovered=[0, 2, 4] marked=[0, 2, 3, 4] ever=[0, 2, 3, 4] samples=82 mode=StopOnError",
    "Hit(100) at Some('r') ends the lesson",
    "after: lessons=1 history=1 accuracy=96.5909 unlocked=Some('t') screen=Typing timed=false save_stats=true save_config=true",
    "after: cursor=0 positions=0 errors=0 correct=0 first=0 recovered=[] marked=[] ever=[] samples=83 mode=StopOnError",
    "a attempts=16 errors=1 recent=15",
    "e attempts=22 errors=0 recent=20",
    "i attempts=6 errors=0 recent=6",
    "l attempts=9 errors=0 recent=9",
    "n attempts=17 errors=0 recent=17",
    "r attempts=17 errors=3 recent=14",
    "active: eniarlt focus: t",
    "next text: tail intent rental taller tire alter anti trailer lantern rat tree tear later letter entire enter internet",
];
