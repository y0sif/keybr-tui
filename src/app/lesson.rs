//! Lesson results, the live per-lesson metrics, and moving from one
//! lesson to the next.

use super::{App, AppScreen};
use crate::engine::LetterFilter;

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

impl App {
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

        // Fold each key's lesson-mean latency into its smoothed time before
        // the scheduler reads confidences — keybr updates stats per result.
        for stats in self.per_key_stats.values_mut() {
            stats.finish_lesson();
        }

        // Update scheduler with current stats (and the drill list, which
        // a newly unlocked letter can widen).
        self.run_scheduler_update();

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
        let filter = LetterFilter::new(&self.scheduler.active_keys, self.effective_focus());
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::unlock_all_letters;
    use crate::engine::filter::{FocusRule, FOCUS_PATTERNS};

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
    fn finishing_a_lesson_refreshes_the_drill_list() {
        // `finish_lesson` runs the scheduler, which can unlock a letter.
        // The debug assertion inside `available_patterns` catches a list
        // left behind; this pins the refresh down in release builds too.
        let mut app = App::new();
        app.alphabet_size = 1.0;
        app.scheduler.alphabet_size = 1.0;
        app.generated_text = "en".to_string();
        app.finish_lesson();
        assert_eq!(app.available_patterns().len(), FOCUS_PATTERNS.len());
    }

    #[test]
    fn pinned_letter_appears_in_every_generated_word() {
        let mut app = App::new();
        app.manual_focus = Some(FocusRule::Key('r'));
        app.start_next_lesson();
        assert!(!app.generated_text.is_empty());
        for word in app.generated_text.split_whitespace() {
            assert!(
                word.contains('r'),
                "pinned 'r' missing from word '{}' in: {}",
                word,
                app.generated_text
            );
        }
    }

    #[test]
    fn pinned_pattern_is_satisfied_by_every_generated_word() {
        for rule in [FocusRule::Contains("cr"), FocusRule::Suffix("ing")] {
            let mut app = App::new();
            unlock_all_letters(&mut app);
            assert!(app.available_patterns().contains(&rule));
            app.manual_focus = Some(rule);
            app.start_next_lesson();
            assert!(!app.generated_text.is_empty());
            for word in app.generated_text.split_whitespace() {
                assert!(
                    rule.matches(word),
                    "pinned {} not satisfied by word '{}' in: {}",
                    rule.config_value(),
                    word,
                    app.generated_text
                );
            }
        }
    }
}
