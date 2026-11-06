//! Typing screen key handling and the per-keystroke lesson accounting.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{request_config_save, request_stats_save};
use crate::app::{App, AppScreen, ErrorMode};
use crate::metrics::{MAX_SAMPLE_MS, MIN_SAMPLE_MS};
use crate::persistence::today_date_string;

/// Increment `today_seconds_practiced` by `lesson_seconds`, after checking
/// for a day-rollover that may have happened mid-session.
/// Tracking in seconds (display as minutes) avoids the sub-minute floor-to-zero
/// that made the daily-goal bar appear stuck at 0.
fn tick_daily_goal(app: &mut App, lesson_seconds: u32) {
    let today = today_date_string();
    if app.today_date != today {
        app.today_date = today;
        app.today_seconds_practiced = 0;
    }
    app.today_seconds_practiced = app.today_seconds_practiced.saturating_add(lesson_seconds);
}

pub(super) fn handle_typing_key(app: &mut App, key: KeyEvent) {
    use KeyCode::*;

    match key.code {
        Esc => {
            // Esc during typing goes to menu, not quit
            request_stats_save(app);
            app.screen = AppScreen::Menu;
        }

        // Toggle error mode
        Tab => {
            app.error_mode = match app.error_mode {
                ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
            };
            request_config_save(app);
        }

        // Backspace — move cursor back and clear error mark
        Backspace if app.cursor_pos > 0 => {
            app.cursor_pos -= 1;
            app.error_positions.remove(&app.cursor_pos);
            app.first_attempt_correct.remove(&app.cursor_pos);
            app.recovered_positions.remove(&app.cursor_pos);
            app.key_target_start = Some(Instant::now());
        }

        // Typing input — includes space
        Char(typed) => {
            handle_typed_char(app, typed);
        }

        _ => {}
    }
}

pub(super) fn handle_typed_char(app: &mut App, typed: char) {
    if app.cursor_pos >= app.generated_text.chars().count() {
        return;
    }

    // Start lesson timer on first keystroke
    if app.lesson_start.is_none() {
        app.lesson_start = Some(Instant::now());
        app.key_target_start = Some(Instant::now());
    }

    let target = match app.generated_text.chars().nth(app.cursor_pos) {
        Some(c) => c,
        None => return,
    };

    let reaction_ms = app
        .key_target_start
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);

    if typed == target {
        let pos = app.cursor_pos;

        if app.ever_error_positions.contains(&pos) {
            app.recovered_positions.insert(pos);
        } else {
            app.first_attempt_correct.insert(pos);
        }

        if target != ' ' {
            if app.first_attempt_correct.contains(&pos) {
                let stats = app.per_key_stats.entry(target).or_default();
                if (MIN_SAMPLE_MS..=MAX_SAMPLE_MS).contains(&reaction_ms) {
                    stats.record_hit(reaction_ms);
                }
            }
            app.lesson_positions += 1;
        }
        app.lesson_correct += 1;
        app.cursor_pos += 1;
        app.key_target_start = Some(Instant::now());

        if app.cursor_pos >= app.generated_text.chars().count() {
            let lesson_seconds = app
                .lesson_start
                .map(|s| s.elapsed().as_secs() as u32)
                .unwrap_or(0);
            app.finish_lesson();
            tick_daily_goal(app, lesson_seconds);
            request_stats_save(app);
        }
    } else {
        let pos = app.cursor_pos;

        if target != ' ' {
            let stats = app.per_key_stats.entry(target).or_default();
            stats.record_error();
            if !app.error_positions.contains(&pos) {
                app.lesson_positions += 1;
                app.lesson_errors += 1;
            }
        }

        app.ever_error_positions.insert(pos);

        match app.error_mode {
            ErrorMode::ForgiveMistakes => {
                app.error_positions.insert(pos);
                app.cursor_pos += 1;
                app.key_target_start = Some(Instant::now());

                if app.cursor_pos >= app.generated_text.chars().count() {
                    let lesson_seconds = app
                        .lesson_start
                        .map(|s| s.elapsed().as_secs() as u32)
                        .unwrap_or(0);
                    app.finish_lesson();
                    tick_daily_goal(app, lesson_seconds);
                    request_stats_save(app);
                }
            }
            ErrorMode::StopOnError => {
                app.error_positions.insert(pos);
                app.key_target_start = Some(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::events::AppEvent;
    use crate::update::test_support::{make_key, make_test_app};
    use crate::update::update;

    #[test]
    fn backspace_decrements_cursor() {
        let mut app = make_test_app("abc");
        app.cursor_pos = 2;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert_eq!(app.cursor_pos, 1);
    }

    #[test]
    fn backspace_does_not_go_below_zero() {
        let mut app = make_test_app("abc");
        app.cursor_pos = 0;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn backspace_removes_error_mark() {
        let mut app = make_test_app("abc");
        app.cursor_pos = 2;
        app.error_positions.insert(1);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert_eq!(app.cursor_pos, 1);
        assert!(!app.error_positions.contains(&1));
    }

    #[test]
    fn backspace_resets_key_target_start() {
        let mut app = make_test_app("abc");
        app.cursor_pos = 2;
        app.key_target_start = None;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert!(app.key_target_start.is_some());
    }

    #[test]
    fn correct_char_marks_first_attempt() {
        let mut app = make_test_app("ab");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert!(app.first_attempt_correct.contains(&0));
        assert!(!app.recovered_positions.contains(&0));
    }

    #[test]
    fn error_then_backspace_then_correct_marks_recovered() {
        let mut app = make_test_app("abc");
        app.error_mode = ErrorMode::ForgiveMistakes;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('x'))));
        assert_eq!(app.cursor_pos, 1);
        assert!(app.error_positions.contains(&0));
        assert!(app.ever_error_positions.contains(&0));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert_eq!(app.cursor_pos, 0);
        assert!(!app.error_positions.contains(&0));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert!(app.recovered_positions.contains(&0));
        assert!(!app.first_attempt_correct.contains(&0));
    }

    #[test]
    fn stop_on_error_does_not_advance_cursor() {
        let mut app = make_test_app("ab");
        app.error_mode = ErrorMode::StopOnError;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('x'))));
        assert_eq!(app.cursor_pos, 0);
        assert!(app.error_positions.contains(&0));
    }

    #[test]
    fn forgive_mistakes_advances_cursor_on_error() {
        let mut app = make_test_app("ab");
        app.error_mode = ErrorMode::ForgiveMistakes;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('x'))));
        assert_eq!(app.cursor_pos, 1);
        assert!(app.error_positions.contains(&0));
    }

    #[test]
    fn backspace_clears_first_attempt_and_recovered() {
        let mut app = make_test_app("ab");
        app.cursor_pos = 1;
        app.first_attempt_correct.insert(0);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert!(!app.first_attempt_correct.contains(&0));
    }

    #[test]
    fn toggle_error_mode() {
        let mut app = make_test_app("ab");
        app.error_mode = ErrorMode::ForgiveMistakes;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Tab)));
        assert_eq!(app.error_mode, ErrorMode::StopOnError);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Tab)));
        assert_eq!(app.error_mode, ErrorMode::ForgiveMistakes);
    }

    #[test]
    fn esc_during_typing_goes_to_menu() {
        let mut app = make_test_app("abc");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
        assert!(app.running);
    }

    #[test]
    fn finishing_lesson_stays_on_typing_screen() {
        // Typing the last char of a fragment should keep us on the Typing
        // screen — no separate summary screen anymore — and immediately
        // regenerate a new fragment.
        let mut app = make_test_app("a");
        let old_text = app.generated_text.clone();
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert_eq!(app.screen, AppScreen::Typing);
        assert!(app.last_lesson.is_some());
        assert_ne!(app.generated_text, old_text);
        // The cursor must be reset for the new fragment.
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn finishing_lesson_does_not_require_extra_keystroke() {
        // Verify that the next fragment is ready to be typed straight away,
        // no intervening "press any key" intercept.
        let mut app = make_test_app("a");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert_eq!(app.screen, AppScreen::Typing);
        // The first character of the new fragment is the live target —
        // it must be a real char (not empty).
        assert!(!app.generated_text.is_empty());
    }

    // --- Deferred persistence: update must only raise flags, never write ---

    #[test]
    fn error_mode_toggle_raises_config_save_flag() {
        let mut app = make_test_app("ab");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Tab)));
        assert!(app.pending_config_save);
    }

    #[test]
    fn finishing_lesson_raises_stats_save_flag() {
        let mut app = make_test_app("a");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert!(app.pending_stats_save);
    }

    #[test]
    fn esc_from_typing_raises_stats_save_flag() {
        let mut app = make_test_app("abc");
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
        assert!(app.pending_stats_save);
    }

    // --- Reaction-time samples: only hits inside 40..=12000 ms count ---

    /// Press `a`, the first target of a lesson already under way, `ms`
    /// after it became the target. The clock only moves forward between
    /// here and the press, so the measured time is `ms` or a little more.
    fn hit_a_after(ms: u64) -> App {
        let mut app = make_test_app("ab");
        app.key_target_start = Some(
            Instant::now()
                .checked_sub(Duration::from_millis(ms))
                .expect("the monotonic clock has run this long"),
        );
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        // Timed or not, the hit is a first-try position of the lesson.
        assert_eq!(app.cursor_pos, 1);
        assert!(app.first_attempt_correct.contains(&0));
        assert_eq!(app.lesson_positions, 1);
        assert_eq!(app.lesson_errors, 0);
        app
    }

    #[test]
    fn a_hit_after_100ms_records_a_time_sample() {
        let app = hit_a_after(100);
        let stats = &app.per_key_stats[&'a'];
        assert_eq!(stats.attempts, 1);
        assert_eq!(stats.errors, 0);
        assert_eq!(stats.reaction_times_ms.len(), 1);
        assert!(stats.reaction_times_ms[0] >= 100);
    }

    /// A hit outside the window records nothing for the key, not even an
    /// attempt: `record_hit` is what counts the attempt.
    #[test]
    fn a_hit_after_20s_records_no_sample() {
        let app = hit_a_after(20_000);
        let stats = &app.per_key_stats[&'a'];
        assert_eq!(stats.attempts, 0);
        assert_eq!(stats.errors, 0);
        assert!(stats.reaction_times_ms.is_empty());
    }

    /// The floor is inclusive. The measured time can only come out at 40
    /// ms or above, so this holds however slow the test machine is.
    #[test]
    fn a_hit_at_the_40ms_floor_records_a_sample() {
        let app = hit_a_after(40);
        let stats = &app.per_key_stats[&'a'];
        assert_eq!(stats.attempts, 1);
        assert_eq!(stats.reaction_times_ms.len(), 1);
    }

    /// The cap itself (exactly 12000 ms) cannot be pinned: the press is
    /// measured a moment after the start is set, so it can read 12001.
    /// One past the cap reads 12001 or more and is always rejected.
    #[test]
    fn a_hit_just_past_the_12s_cap_records_no_sample() {
        let app = hit_a_after(12_001);
        let stats = &app.per_key_stats[&'a'];
        assert_eq!(stats.attempts, 0);
        assert!(stats.reaction_times_ms.is_empty());
    }
}
