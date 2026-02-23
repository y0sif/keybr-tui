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

pub struct App {
    pub running: bool,

    // --- Text state ---
    pub generated_text: String,
    /// Index of the current target character in `generated_text`.
    pub cursor_pos: usize,
    /// Indices of characters that were typed incorrectly.
    pub error_positions: HashSet<usize>,

    // --- Metrics ---
    pub per_key_stats: HashMap<char, KeyStats>,
    /// When did the current target character become active (for reaction timing).
    pub key_target_start: Option<Instant>,
    /// When did the user first start typing in this session.
    pub session_start: Option<Instant>,
    /// Total correctly typed characters this session (for WPM).
    pub correct_chars: u32,

    // --- Engine ---
    pub scheduler: LetterScheduler,

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Proficiency threshold: average reaction time in ms to unlock a new letter.
    pub target_speed_ms: u64,
}

impl App {
    pub fn new() -> Self {
        let scheduler = LetterScheduler::new();
        let stats: HashMap<char, KeyStats> = HashMap::new();
        let text = generate_text(&scheduler.active_keys, &stats);

        App {
            running: true,
            generated_text: text,
            cursor_pos: 0,
            error_positions: HashSet::new(),
            per_key_stats: stats,
            key_target_start: None,
            session_start: None,
            correct_chars: 0,
            scheduler,
            error_mode: ErrorMode::MoveOn,
            target_speed_ms: 400,
        }
    }

    /// Current WPM (words = correct_chars / 5).
    pub fn wpm(&self) -> f64 {
        let start = match self.session_start {
            Some(s) => s,
            None => return 0.0,
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        if elapsed_secs < 1.0 {
            return 0.0;
        }
        (self.correct_chars as f64 / 5.0) / (elapsed_secs / 60.0)
    }

    /// Overall accuracy this session.
    pub fn accuracy(&self) -> f64 {
        let total_attempts: u32 = self.per_key_stats.values().map(|s| s.attempts).sum();
        let total_errors: u32 = self.per_key_stats.values().map(|s| s.errors).sum();
        if total_attempts == 0 {
            return 100.0;
        }
        ((total_attempts - total_errors) as f64 / total_attempts as f64) * 100.0
    }

    /// Regenerate text and check for key unlocks after finishing a batch.
    pub fn advance_batch(&mut self) {
        self.scheduler
            .try_unlock(&self.per_key_stats, self.target_speed_ms);
        self.generated_text =
            generate_text(&self.scheduler.active_keys, &self.per_key_stats);
        self.cursor_pos = 0;
        self.error_positions.clear();
        self.key_target_start = Some(Instant::now());
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
