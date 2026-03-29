use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::config::{Config, ErrorModeSerde};
use crate::engine::{LetterFilter, LetterScheduler, WordGenerator};
use crate::metrics::KeyStats;
use crate::persistence::{SavedKeyStats, SavedStats};

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
    LessonSummary,
    Progress,
    Settings,
}

/// Results stored after a lesson completes.
pub struct LessonResult {
    pub wpm: f64,
    pub accuracy: f64,
    /// Letter that was unlocked at the end of this lesson, if any.
    pub newly_unlocked: Option<char>,
    /// Top weakest keys with their confidence levels (sorted weakest first).
    pub weakest_keys: Vec<(char, f64)>,
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
        }

        // Initial scheduler update to set focused key
        scheduler.update(&stats, target_cpm);

        let filter = LetterFilter::new(&scheduler.active_keys, scheduler.focused_key);
        let mut generator = WordGenerator::new();
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
            last_lesson: None,
            lesson_count,
            scheduler,
            generator,
            error_mode,
            target_cpm,
            fragment_length: 100,
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

        // Compute weakest keys (lowest confidence among active keys)
        let mut key_confidences: Vec<(char, f64)> = self
            .scheduler
            .active_keys
            .iter()
            .filter_map(|&k| {
                self.per_key_stats
                    .get(&k)
                    .map(|s| (k, s.confidence(self.target_cpm)))
            })
            .collect();
        key_confidences.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let weakest_keys: Vec<(char, f64)> = key_confidences.into_iter().take(5).collect();

        self.last_lesson = Some(LessonResult {
            wpm,
            accuracy,
            newly_unlocked,
            weakest_keys,
        });
        self.screen = AppScreen::LessonSummary;
    }

    /// Called when the user dismisses the lesson summary (any key).
    /// Generates new text and returns to the typing screen.
    pub fn start_next_lesson(&mut self) {
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
            version: 1,
            keys,
            unlocked_letters: self.scheduler.active_keys.clone(),
            total_lessons: self.lesson_count,
            last_session: chrono_now_iso8601(),
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
