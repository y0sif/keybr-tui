use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::engine::{generate_text, LetterScheduler};
use crate::metrics::KeyStats;

#[derive(Clone, Copy, PartialEq, Eq)]
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

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Target typing speed in WPM. Used as the proficiency unlock threshold.
    pub target_wpm: u32,
}

impl App {
    pub fn new() -> Self {
        let scheduler = LetterScheduler::new();
        let stats: HashMap<char, KeyStats> = HashMap::new();
        let text = generate_text(&scheduler.active_keys, &stats);

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
            error_mode: ErrorMode::MoveOn,
            target_wpm: 30,
        }
    }

    /// Convert the WPM goal to a per-key reaction time threshold in ms.
    /// At W WPM, each character takes 12000/W milliseconds on average.
    pub fn target_speed_ms(&self) -> u64 {
        12000 / self.target_wpm as u64
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
    /// Saves lesson results, checks for unlocks, transitions to summary screen.
    pub fn finish_lesson(&mut self) {
        let wpm = self.lesson_wpm();
        let accuracy = self.lesson_accuracy();
        let newly_unlocked = self
            .scheduler
            .try_unlock(&self.per_key_stats, self.target_speed_ms());

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
        self.generated_text =
            generate_text(&self.scheduler.active_keys, &self.per_key_stats);
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
