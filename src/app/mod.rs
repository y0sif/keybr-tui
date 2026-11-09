use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::engine::filter::FocusRule;
use crate::engine::{Language, LetterFilter, LetterScheduler, WordGenerator};
use crate::metrics::KeyStats;
use crate::persistence::{today_date_string, SavedStats};

mod focus;
mod lesson;
mod profile;
#[cfg(test)]
mod test_support;

use focus::compute_available_patterns;
pub use lesson::{lesson_score, LessonResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorMode {
    /// Must fix errors before continuing (backspace required).
    StopOnError,
    /// Can continue past errors, backspace optional.
    ForgiveMistakes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Menu,
    Typing,
    Progress,
    Settings,
}

pub struct App {
    pub running: bool,
    pub screen: AppScreen,

    // --- Text state ---
    pub generated_text: String,
    /// Index of the current target character in `generated_text`.
    pub cursor_pos: usize,
    /// Indices of characters that were typed incorrectly.
    pub error_positions: HashSet<usize>,
    /// Characters that were typed correctly on first attempt (no backspace correction).
    pub first_attempt_correct: HashSet<usize>,
    /// Characters that were corrected after an error (was wrong, then fixed).
    pub recovered_positions: HashSet<usize>,
    /// Positions that have ever been in error (tracks history, not cleared by backspace).
    pub ever_error_positions: HashSet<usize>,

    // --- Per-lesson metrics (reset each lesson) ---
    /// When the first key of the current lesson was pressed.
    pub lesson_start: Option<Instant>,
    /// Correctly typed characters this lesson (used for WPM).
    pub lesson_correct: u32,
    /// Total positions attempted this lesson (for accuracy denominator).
    pub lesson_positions: u32,
    /// Positions that had an error (first-try errors).
    pub lesson_errors: u32,

    // --- Cumulative per-key stats (persist across lessons) ---
    pub per_key_stats: HashMap<char, KeyStats>,
    /// When did the current target char become active (for reaction timing).
    pub key_target_start: Option<Instant>,

    // --- Last lesson's results (shown in stats bar during next lesson) ---
    pub last_lesson: Option<LessonResult>,
    /// Rolling in-memory history of completed lessons (capped).
    /// Not persisted — used only to compute deltas vs running average
    /// in the dashboard. Older entries fall off the front.
    pub lesson_history: Vec<LessonResult>,

    /// Number of lessons completed in the current session.
    pub lesson_count: u32,

    // --- Engine ---
    /// Practice language: alphabet, phonetic model, word list and which
    /// stats file this App reads and writes. Fixed for the life of the
    /// App: switching language in Settings replaces the whole App (see
    /// `switched_to`), so nothing derived from it can be left behind.
    pub language: Language,
    pub scheduler: LetterScheduler,
    pub generator: WordGenerator,

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Target typing speed in CPM. Internally everything uses CPM.
    /// Display as WPM = CPM / 5.
    pub target_cpm: f64,
    /// Fragment length for text generation.
    pub fragment_length: usize,
    /// When true, lessons draw from real dictionary words that fit the
    /// active letter filter, padded with phonetic words when fewer than
    /// 15 fit (as on keybr.com).
    pub natural_words: bool,
    /// Daily practice goal in minutes. 0 hides the daily-goal indicator.
    pub daily_goal_minutes: u32,
    /// Fraction of the non-starter alphabet to force-include regardless of
    /// confidence (keybr's `alphabetSize`, in [0.0, 1.0]). Mirrored onto
    /// `scheduler.alphabet_size` so the next scheduler update applies it.
    pub alphabet_size: f64,
    /// User-pinned focus rule (keybr's manual lesson focus, widened to
    /// cover combination drills). Overrides the scheduler's auto pick at
    /// generation time only: scheduler logic, stats recording, and unlock
    /// progression are untouched.
    ///
    /// One field holds both kinds of pin on purpose. The settings screen
    /// offers a letter row and a pattern row, and they write here, so a
    /// letter pin and a pattern pin cannot coexist by construction rather
    /// than by convention.
    pub manual_focus: Option<FocusRule>,
    /// A `focus_letter` from config.toml that this session cannot use but
    /// another language may have set, since every language shares the
    /// key. Never applied; held as read so `to_config` writes it back
    /// instead of erasing it. Set by `set_manual_focus_from_config`, and
    /// by `switched_to` for a pin the language switched from left in the
    /// file. Cleared when the user picks a pin in Settings.
    pub foreign_focus_letter: Option<char>,
    /// Same as `foreign_focus_letter`, for `focus_pattern`.
    pub foreign_focus_pattern: Option<String>,
    /// The combination drills the settings pattern row may offer right
    /// now, derived from the unlocked alphabet.
    ///
    /// Stored rather than computed on demand because deriving it scans the
    /// whole wordlist once per drill, and the settings screen and the
    /// taria tree would otherwise do that on every frame. It is also the
    /// one definition of "available", read by `effective_focus`, the
    /// settings row, and the pattern cycler alike, so the UI cannot show a
    /// drill the generator is not running.
    ///
    /// Private so `refresh_available_patterns` is the only writer, and
    /// read through `available_patterns()`. Refresh after anything that
    /// can change `scheduler.active_keys`, which in practice means after
    /// every `scheduler.update` (see `run_scheduler_update`) and after
    /// restoring saved letters at construction.
    available_patterns: Vec<FocusRule>,
    /// `scheduler.active_keys.len()` when `available_patterns` was last
    /// computed, so debug builds can catch a stale list. Not a general
    /// cache key: active keys only ever grow at runtime, so the length
    /// pins the alphabet down well enough to fail a test that forgets to
    /// refresh.
    available_patterns_for_keys: usize,

    // --- Daily-goal tracker (persisted) ---
    /// Wall-clock seconds practiced today. Display as minutes; storing in
    /// seconds avoids the floor-to-zero on sub-minute lessons.
    pub today_seconds_practiced: u32,
    /// YYYY-MM-DD this counter refers to. Reset on day rollover.
    pub today_date: String,

    // --- Navigation state ---
    /// Selected item index in the main menu.
    pub menu_selection: usize,
    /// Selected item index in the settings screen.
    pub settings_selection: usize,
    /// First letter row the Progress table shows, when the language has
    /// more letters than the terminal has rows for. `update` clamps it to
    /// `crate::screens::progress::scroll_limits` before every step, and
    /// `render` clamps it again, since a resize that makes room for more
    /// rows lowers the limit between keys.
    pub progress_scroll: usize,
    /// Terminal height in rows, set by `main` at launch and by `update` on
    /// every resize. Only the Progress scroll clamp reads it, since
    /// `update` has no frame to measure.
    pub terminal_height: u16,

    // --- Deferred persistence (MVU purity) ---
    /// Set by `update` when stats should be written; main's event loop
    /// clears the flag and queues the write on the background writer
    /// (`persistence::Saver`). `update` itself never does disk I/O, so
    /// tests driving it can't touch the user's real files.
    pub pending_stats_save: bool,
    /// Same as `pending_stats_save`, for the config file.
    pub pending_config_save: bool,
    /// Set by `update` when the user steps the Settings language row;
    /// `flush_pending_saves` performs the switch, which saves this
    /// language's stats and loads the new one's, and clears it. `None`
    /// means no switch is owed, and so does the current language, which
    /// `update` never leaves here.
    pub pending_language_switch: Option<Language>,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_opts(35, ErrorMode::ForgiveMistakes) // 35 WPM = 175 CPM
    }

    pub fn new_with_opts(target_wpm: u32, error_mode: ErrorMode) -> Self {
        Self::new_with_state(target_wpm, error_mode, None)
    }

    /// Create a new English App, optionally restoring state from saved stats.
    pub fn new_with_state(
        target_wpm: u32,
        error_mode: ErrorMode,
        saved: Option<SavedStats>,
    ) -> Self {
        Self::new_with_language(target_wpm, error_mode, saved, Language::English)
    }

    /// Create a new App practicing `language`, optionally restoring state
    /// from that language's saved stats.
    pub fn new_with_language(
        target_wpm: u32,
        error_mode: ErrorMode,
        saved: Option<SavedStats>,
        language: Language,
    ) -> Self {
        let mut scheduler = LetterScheduler::for_language(language);
        let mut stats: HashMap<char, KeyStats> = HashMap::new();
        let target_cpm = target_wpm as f64 * 5.0;
        let mut lesson_count: u32 = 0;
        let today = today_date_string();
        let mut today_seconds_practiced: u32 = 0;
        let mut today_date: String = today.clone();
        let mut last_lesson: Option<LessonResult> = None;
        let mut lesson_history: Vec<LessonResult> = Vec::new();

        // Restore from saved stats if available
        if let Some(saved) = saved {
            // Restore per-key stats
            for (ch, saved_key) in &saved.keys {
                stats.insert(*ch, KeyStats::from(saved_key));
            }

            // Restore unlocked letters into scheduler (letters outside the
            // language's alphabet are dropped; too few left keeps the
            // starter set, as a short list always did).
            scheduler.restore_active_keys(saved.unlocked_letters);

            lesson_count = saved.total_lessons;

            // Restore daily-goal counter (with day-rollover protection).
            // `load()` already normalises on read, but be defensive in case
            // a caller hands us a raw SavedStats from somewhere else.
            if saved.today_date == today && !today.is_empty() {
                today_seconds_practiced = saved.today_seconds_practiced;
                today_date = saved.today_date;
            } else {
                today_seconds_practiced = 0;
                today_date = today.clone();
            }

            // Restore lesson stats so the dashboard's Metrics row shows
            // the previous session's numbers (and deltas) on launch.
            // `newly_unlocked` is intentionally not persisted — the
            // unlock callout is a one-time celebration, not state.
            last_lesson = saved.last_lesson.map(|r| LessonResult {
                wpm: r.wpm,
                accuracy: r.accuracy,
                newly_unlocked: None,
            });
            lesson_history = saved
                .lesson_history
                .into_iter()
                .map(|r| LessonResult {
                    wpm: r.wpm,
                    accuracy: r.accuracy,
                    newly_unlocked: None,
                })
                .collect();
        }

        // Initial scheduler update to set focused key
        scheduler.update(&stats, target_cpm);

        let filter = LetterFilter::new(
            &scheduler.active_keys,
            scheduler.focused_key.map(FocusRule::Key),
        );
        let mut generator = WordGenerator::for_language(language);
        // Default to the natural-words blend on; `apply_config` overrides
        // this from the loaded config immediately after construction.
        generator.set_natural_words(true);
        let text = generator.generate_fragment(&filter, 100);

        // Seed the drill list for the alphabet we just restored.
        // `apply_config` re-runs the scheduler with the configured
        // `alphabet_size` and refreshes this again before anything reads it.
        let available_patterns =
            compute_available_patterns(language, &generator, &scheduler.active_keys);
        let available_patterns_for_keys = scheduler.active_keys.len();

        App {
            running: true,
            screen: AppScreen::Menu,
            generated_text: text,
            cursor_pos: 0,
            error_positions: HashSet::new(),
            first_attempt_correct: HashSet::new(),
            recovered_positions: HashSet::new(),
            ever_error_positions: HashSet::new(),
            lesson_start: None,
            lesson_correct: 0,
            lesson_positions: 0,
            lesson_errors: 0,
            per_key_stats: stats,
            key_target_start: None,
            last_lesson,
            lesson_history,
            lesson_count,
            language,
            scheduler,
            generator,
            error_mode,
            target_cpm,
            fragment_length: 100,
            natural_words: true,
            daily_goal_minutes: 30,
            alphabet_size: 0.0,
            manual_focus: None,
            foreign_focus_letter: None,
            foreign_focus_pattern: None,
            available_patterns,
            available_patterns_for_keys,
            today_seconds_practiced,
            today_date,
            menu_selection: 0,
            settings_selection: 0,
            progress_scroll: 0,
            // A VT100 screen until `main` reports the real size.
            terminal_height: 24,
            pending_stats_save: false,
            pending_config_save: false,
            pending_language_switch: None,
        }
    }

    /// Target WPM for display (WPM = CPM / 5).
    pub fn target_wpm(&self) -> u32 {
        (self.target_cpm / 5.0).round() as u32
    }

    /// Set the target via WPM (converts to CPM internally).
    pub fn set_target_wpm(&mut self, wpm: u32) {
        self.target_cpm = wpm as f64 * 5.0;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::set_active_keys;
    use super::*;

    #[test]
    fn target_wpm_conversion() {
        let app = App::new_with_opts(35, ErrorMode::ForgiveMistakes);
        assert_eq!(app.target_wpm(), 35);
        assert!((app.target_cpm - 175.0).abs() < f64::EPSILON);
    }

    #[test]
    fn new_with_opts_sets_values() {
        let app = App::new_with_opts(50, ErrorMode::StopOnError);
        assert_eq!(app.target_wpm(), 50);
        assert_eq!(app.error_mode, ErrorMode::StopOnError);
    }

    #[test]
    fn default_target_is_35_wpm() {
        let app = App::new();
        assert_eq!(app.target_wpm(), 35);
        assert!((app.target_cpm - 175.0).abs() < f64::EPSILON);
    }

    #[test]
    fn turkish_app_starts_with_turkish_letters_and_no_drills() {
        let mut app =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        assert_eq!(
            app.scheduler.active_keys,
            vec!['a', 'e', 'i', 'n', 'ı', 'r']
        );
        assert!(app.available_patterns().is_empty());
        for c in app.generated_text.chars() {
            assert!(c == ' ' || app.scheduler.active_keys.contains(&c), "{c}");
        }
        // Unlock everything: still no English drills on offer.
        set_active_keys(&mut app, Language::Turkish.unlock_order().to_vec());
        assert!(app.available_patterns().is_empty());
        app.start_next_lesson();
        assert!(app.generated_text.chars().count() >= app.fragment_length);
    }
}
