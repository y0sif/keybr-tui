use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::engine::{LetterFilter, LetterScheduler, WordGenerator};
use crate::metrics::KeyStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorMode {
    /// Cursor advances even on wrong key; character is marked red.
    MoveOn,
    /// Cursor stays until the correct key is pressed.
    StopOnError,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Typing,
    LessonSummary,
}

/// Results stored after a lesson completes.
pub struct LessonResult {
    pub wpm: f64,
    pub accuracy: f64,
    /// Letter that was unlocked at the end of this lesson, if any.
    pub newly_unlocked: Option<char>,
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

    // --- Engine ---
    pub scheduler: LetterScheduler,
    pub generator: WordGenerator,

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Target typing speed in CPM. Internally everything uses CPM.
    /// Display as WPM = CPM / 5.
    pub target_cpm: f64,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_opts(35, ErrorMode::MoveOn) // 35 WPM = 175 CPM
    }

    pub fn new_with_opts(target_wpm: u32, error_mode: ErrorMode) -> Self {
        let mut scheduler = LetterScheduler::new();
        let stats: HashMap<char, KeyStats> = HashMap::new();
        let target_cpm = target_wpm as f64 * 5.0;

        // Initial scheduler update to set focused key
        scheduler.update(&stats, target_cpm);

        let filter = LetterFilter::new(&scheduler.active_keys, scheduler.focused_key);
        let mut generator = WordGenerator::new();
        let text = generator.generate_fragment(&filter, 100);

        App {
            running: true,
            screen: AppScreen::Typing,
            generated_text: text,
            cursor_pos: 0,
            error_positions: HashSet::new(),
            lesson_start: None,
            lesson_correct: 0,
            lesson_positions: 0,
            lesson_errors: 0,
            per_key_stats: stats,
            key_target_start: None,
            last_lesson: None,
            scheduler,
            generator,
            error_mode,
            target_cpm,
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

    /// WPM for the current lesson so far (live, used only internally).
    pub fn lesson_wpm(&self) -> f64 {
        let start = match self.lesson_start {
            Some(s) => s,
            None => return 0.0,
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        if elapsed_secs < 1.0 {
            return 0.0;
        }
        (self.lesson_correct as f64 / 5.0) / (elapsed_secs / 60.0)
    }

    /// Accuracy for the current lesson so far.
    pub fn lesson_accuracy(&self) -> f64 {
        if self.lesson_positions == 0 {
            return 100.0;
        }
        ((self.lesson_positions - self.lesson_errors) as f64 / self.lesson_positions as f64)
            * 100.0
    }

    /// Called when the user finishes typing all chars in the current batch.
    /// Saves lesson results, runs scheduler, transitions to summary screen.
    pub fn finish_lesson(&mut self) {
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

        self.last_lesson = Some(LessonResult {
            wpm,
            accuracy,
            newly_unlocked,
        });
        self.screen = AppScreen::LessonSummary;
    }

    /// Called when the user dismisses the lesson summary (any key).
    /// Generates new text and returns to the typing screen.
    pub fn start_next_lesson(&mut self) {
        let filter = LetterFilter::new(&self.scheduler.active_keys, self.scheduler.focused_key);
        self.generated_text = self.generator.generate_fragment(&filter, 100);
        self.cursor_pos = 0;
        self.error_positions.clear();
        self.key_target_start = None;
        self.lesson_start = None;
        self.lesson_correct = 0;
        self.lesson_positions = 0;
        self.lesson_errors = 0;
        self.screen = AppScreen::Typing;
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
        let app = App::new_with_opts(35, ErrorMode::MoveOn);
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
