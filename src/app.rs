use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::config::{Config, ErrorModeSerde};
use crate::engine::{LetterFilter, LetterScheduler, WordGenerator};
use crate::metrics::KeyStats;
use crate::persistence::{today_date_string, SavedKeyStats, SavedLessonResult, SavedStats};

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

/// Results stored after a lesson completes.
#[derive(Debug, Clone)]
pub struct LessonResult {
    pub wpm: f64,
    pub accuracy: f64,
    /// Letter that was unlocked at the end of this lesson, if any.
    pub newly_unlocked: Option<char>,
}

/// Approximate keybr-style score from a lesson's wpm + accuracy.
/// Real keybr weights speed and accuracy together; we use a simple
/// product that rewards both — keep this in sync with any deltas
/// displayed in the dashboard.
pub fn lesson_score(wpm: f64, accuracy: f64) -> f64 {
    let acc_ratio = (accuracy / 100.0).clamp(0.0, 1.0);
    (wpm * acc_ratio * acc_ratio * 100.0).round()
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
    pub scheduler: LetterScheduler,
    pub generator: WordGenerator,

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Target typing speed in CPM. Internally everything uses CPM.
    /// Display as WPM = CPM / 5.
    pub target_cpm: f64,
    /// Fragment length for text generation.
    pub fragment_length: usize,
    /// When true, mix real English dictionary words into generated text
    /// (falling back to the phonetic model when no word matches the
    /// active letter filter).
    pub natural_words: bool,
    /// Daily practice goal in minutes. 0 hides the daily-goal indicator.
    pub daily_goal_minutes: u32,

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
}

impl App {
    pub fn new() -> Self {
        Self::new_with_opts(35, ErrorMode::ForgiveMistakes) // 35 WPM = 175 CPM
    }

    pub fn new_with_opts(target_wpm: u32, error_mode: ErrorMode) -> Self {
        Self::new_with_state(target_wpm, error_mode, None)
    }

    /// Create a new App, optionally restoring state from saved stats.
    pub fn new_with_state(
        target_wpm: u32,
        error_mode: ErrorMode,
        saved: Option<SavedStats>,
    ) -> Self {
        let mut scheduler = LetterScheduler::new();
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
                let key_stats = stats.entry(*ch).or_default();
                key_stats.attempts = saved_key.attempts;
                key_stats.errors = saved_key.errors;
                key_stats.filtered_time_ms = saved_key.filtered_time_ms;
                key_stats.best_filtered_time_ms = saved_key.best_filtered_time_ms;
                // Restore recent times (up to 20 most recent, matching KeyStats cap)
                let recent = &saved_key.recent_times_ms;
                let start = recent.len().saturating_sub(20);
                key_stats.reaction_times_ms = recent[start..].to_vec();
            }

            // Restore unlocked letters into scheduler
            if saved.unlocked_letters.len() >= 6 {
                scheduler.active_keys = saved.unlocked_letters;
                // Set unlock_index based on how many keys are active
                scheduler.set_unlock_index_from_active();
            }

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

        let filter = LetterFilter::new(&scheduler.active_keys, scheduler.focused_key);
        let mut generator = WordGenerator::new();
        // Default to the natural-words blend on; main.rs overrides this
        // from the loaded config immediately after construction.
        generator.set_natural_words(true);
        let text = generator.generate_fragment(&filter, 100);

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
            scheduler,
            generator,
            error_mode,
            target_cpm,
            fragment_length: 100,
            natural_words: true,
            daily_goal_minutes: 30,
            today_seconds_practiced,
            today_date,
            menu_selection: 0,
            settings_selection: 0,
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

    /// WPM for the current lesson so far.
    /// Only first-attempt correct characters count toward speed.
    pub fn lesson_wpm(&self) -> f64 {
        let start = match self.lesson_start {
            Some(s) => s,
            None => return 0.0,
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        if elapsed_secs < 1.0 {
            return 0.0;
        }
        // Use first_attempt_correct count for accurate WPM
        let correct_chars = self.first_attempt_correct.len() as f64;
        (correct_chars / 5.0) / (elapsed_secs / 60.0)
    }

    /// Mean WPM of all lessons in `lesson_history` *excluding* the most
    /// recent one, so deltas computed from this don't self-compare.
    /// Returns `None` when there's no prior lesson to compare against.
    pub fn prev_mean_wpm(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1].iter().map(|r| r.wpm).sum();
        Some(sum / (n - 1) as f64)
    }

    /// Mean accuracy of all lessons *excluding* the most recent one.
    pub fn prev_mean_accuracy(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1]
            .iter()
            .map(|r| r.accuracy)
            .sum();
        Some(sum / (n - 1) as f64)
    }

    /// Mean score (derived from wpm + accuracy) of all lessons except the last.
    pub fn prev_mean_score(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1]
            .iter()
            .map(|r| lesson_score(r.wpm, r.accuracy))
            .sum();
        Some(sum / (n - 1) as f64)
    }

    /// Accuracy for the current lesson so far.
    pub fn lesson_accuracy(&self) -> f64 {
        if self.lesson_positions == 0 {
            return 100.0;
        }
        ((self.lesson_positions - self.lesson_errors) as f64 / self.lesson_positions as f64) * 100.0
    }

    /// Called when the user finishes typing all chars in the current batch.
    /// Saves lesson results, runs scheduler, transitions to summary screen.
    pub fn finish_lesson(&mut self) {
        self.lesson_count += 1;
        let wpm = self.lesson_wpm();
        let accuracy = self.lesson_accuracy();

        let old_count = self.scheduler.active_keys.len();

        // Update scheduler with current stats
        self.scheduler.update(&self.per_key_stats, self.target_cpm);

        let newly_unlocked = if self.scheduler.active_keys.len() > old_count {
            Some(*self.scheduler.active_keys.last().unwrap())
        } else {
            None
        };

        let result = LessonResult {
            wpm,
            accuracy,
            newly_unlocked,
        };
        // Push to in-memory history so the dashboard can compute deltas
        // against the running mean. Cap so a long session can't grow this
        // unboundedly.
        const HISTORY_CAP: usize = 50;
        self.lesson_history.push(result.clone());
        if self.lesson_history.len() > HISTORY_CAP {
            let overflow = self.lesson_history.len() - HISTORY_CAP;
            self.lesson_history.drain(0..overflow);
        }
        self.last_lesson = Some(result);

        // Immediately roll into the next lesson — no separate summary screen.
        // `start_next_lesson` regenerates text, resets per-lesson counters,
        // and sets `screen = AppScreen::Typing`.
        self.start_next_lesson();
    }

    /// Called when the user dismisses the lesson summary (any key).
    /// Generates new text and returns to the typing screen.
    pub fn start_next_lesson(&mut self) {
        // Propagate the current natural-words preference into the
        // generator before regenerating, so config changes take effect
        // at the next lesson boundary.
        self.generator.set_natural_words(self.natural_words);
        let filter = LetterFilter::new(&self.scheduler.active_keys, self.scheduler.focused_key);
        self.generated_text = self
            .generator
            .generate_fragment(&filter, self.fragment_length);
        self.cursor_pos = 0;
        self.error_positions.clear();
        self.first_attempt_correct.clear();
        self.recovered_positions.clear();
        self.ever_error_positions.clear();
        self.key_target_start = None;
        self.lesson_start = None;
        self.lesson_correct = 0;
        self.lesson_positions = 0;
        self.lesson_errors = 0;
        self.screen = AppScreen::Typing;
    }

    /// Convert current app state to a `SavedStats` for persistence.
    pub fn to_saved_stats(&self) -> SavedStats {
        let mut keys = HashMap::new();
        for (ch, key_stats) in &self.per_key_stats {
            // Keep up to 50 recent reaction times for persistence
            let recent: Vec<u64> = key_stats.reaction_times_ms.to_vec();
            keys.insert(
                *ch,
                SavedKeyStats {
                    attempts: key_stats.attempts,
                    errors: key_stats.errors,
                    filtered_time_ms: key_stats.filtered_time_ms,
                    best_filtered_time_ms: key_stats.best_filtered_time_ms,
                    recent_times_ms: recent,
                },
            );
        }

        SavedStats {
            version: 2,
            keys,
            unlocked_letters: self.scheduler.active_keys.clone(),
            total_lessons: self.lesson_count,
            last_session: chrono_now_iso8601(),
            today_seconds_practiced: self.today_seconds_practiced,
            today_minutes_practiced: None,
            today_date: self.today_date.clone(),
            last_lesson: self.last_lesson.as_ref().map(|r| SavedLessonResult {
                wpm: r.wpm,
                accuracy: r.accuracy,
            }),
            lesson_history: self
                .lesson_history
                .iter()
                .map(|r| SavedLessonResult {
                    wpm: r.wpm,
                    accuracy: r.accuracy,
                })
                .collect(),
        }
    }

    /// Convert current app settings to a `Config` for persistence.
    pub fn to_config(&self) -> Config {
        Config {
            target_wpm: self.target_wpm(),
            error_mode: match self.error_mode {
                ErrorMode::ForgiveMistakes => ErrorModeSerde::ForgiveMistakes,
                ErrorMode::StopOnError => ErrorModeSerde::StopOnError,
            },
            fragment_length: self.fragment_length,
            natural_words: self.natural_words,
            daily_goal_minutes: self.daily_goal_minutes,
        }
    }
}

/// Simple ISO 8601 timestamp without depending on chrono.
fn chrono_now_iso8601() -> String {
    // Use std::time to produce a Unix timestamp, format manually
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => {
            let secs = d.as_secs();
            // Approximate: good enough for a "last session" marker
            format!("{secs}")
        }
        Err(_) => "0".to_string(),
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accuracy_is_100_with_no_errors() {
        let mut app = App::new();
        app.lesson_positions = 20;
        app.lesson_errors = 0;
        assert!((app.lesson_accuracy() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn accuracy_reflects_errors() {
        let mut app = App::new();
        app.lesson_positions = 10;
        app.lesson_errors = 2;
        // (10 - 2) / 10 * 100 = 80.0
        assert!((app.lesson_accuracy() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wpm_is_zero_before_lesson_starts() {
        let app = App::new();
        assert!((app.lesson_wpm() - 0.0).abs() < f64::EPSILON);
    }

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
}
