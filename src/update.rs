use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppScreen, ErrorMode};
use crate::events::AppEvent;

/// Save stats to disk, logging errors to stderr without crashing.
fn auto_save_stats(app: &App) {
    let saved = app.to_saved_stats();
    if let Err(e) = saved.save() {
        eprintln!("Warning: failed to save stats: {e}");
    }
}

/// Save config to disk, logging errors to stderr without crashing.
fn auto_save_config(app: &App) {
    let cfg = app.to_config();
    if let Err(e) = cfg.save() {
        eprintln!("Warning: failed to save config: {e}");
    }
}

pub fn update(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Key(key) => handle_key(app, key),
        AppEvent::Tick => {}
        AppEvent::Resize => {}
    }
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    // Global quit — Escape never conflicts with typing
    if key.code == Esc {
        auto_save_stats(app);
        app.running = false;
        return;
    }
    if key.code == Char('c') && key.modifiers == KeyModifiers::CONTROL {
        auto_save_stats(app);
        app.running = false;
        return;
    }

    match app.screen {
        AppScreen::LessonSummary => {
            // Any key starts the next lesson
            app.start_next_lesson();
        }
        AppScreen::Typing => {
            handle_typing_key(app, key);
        }
    }
}

fn handle_typing_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    match key.code {
        // Toggle error mode
        Tab => {
            app.error_mode = match app.error_mode {
                ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
            };
            auto_save_config(app);
        }

        // Adjust WPM goal
        Char('+') | Char('=') => {
            let new_wpm = (app.target_wpm() + 5).min(200);
            app.set_target_wpm(new_wpm);
            auto_save_config(app);
        }
        Char('-') => {
            let new_wpm = app.target_wpm().saturating_sub(5).max(10);
            app.set_target_wpm(new_wpm);
            auto_save_config(app);
        }

        // Backspace — move cursor back and clear error mark
        Backspace => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                // Remove error mark if backing over an error
                app.error_positions.remove(&app.cursor_pos);
                // Remove from first_attempt_correct/recovered since we're re-doing this position
                app.first_attempt_correct.remove(&app.cursor_pos);
                app.recovered_positions.remove(&app.cursor_pos);
                // Reset the key target timer
                app.key_target_start = Some(Instant::now());
            }
        }

        // Typing input — includes space
        Char(typed) => {
            handle_typed_char(app, typed);
        }

        _ => {}
    }
}

fn handle_typed_char(app: &mut App, typed: char) {
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
        // Correct keystroke
        let pos = app.cursor_pos;

        // Track first-attempt vs recovery
        if app.ever_error_positions.contains(&pos) {
            // This position had an error at some point — it's a recovery
            app.recovered_positions.insert(pos);
        } else {
            // Never had an error — first-attempt correct
            app.first_attempt_correct.insert(pos);
        }

        // Record stats for letters (not spaces), only for first-attempt correct
        if target != ' ' {
            if app.first_attempt_correct.contains(&pos) {
                let stats = app.per_key_stats.entry(target).or_default();
                // Validate reaction time: reject < 40ms or > 12000ms (keybr's bounds)
                if (40..=12000).contains(&reaction_ms) {
                    stats.record_hit(reaction_ms);
                }
            }
            app.lesson_positions += 1;
        }
        app.lesson_correct += 1;
        app.cursor_pos += 1;
        app.key_target_start = Some(Instant::now());

        // Check if we've finished the current lesson
        if app.cursor_pos >= app.generated_text.chars().count() {
            app.finish_lesson();
            auto_save_stats(app);
        }
    } else {
        // Wrong keystroke
        let pos = app.cursor_pos;

        if target != ' ' {
            let stats = app.per_key_stats.entry(target).or_default();
            stats.record_error();
            // Only count first-try errors (don't double-count in StopOnError mode)
            if !app.error_positions.contains(&pos) {
                app.lesson_positions += 1;
                app.lesson_errors += 1;
            }
        }

        // Mark as having had an error (permanent, survives backspace)
        app.ever_error_positions.insert(pos);

        match app.error_mode {
            ErrorMode::ForgiveMistakes => {
                app.error_positions.insert(pos);
                app.cursor_pos += 1;
                app.key_target_start = Some(Instant::now());

                if app.cursor_pos >= app.generated_text.chars().count() {
                    app.finish_lesson();
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
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_test_app(text: &str) -> App {
        let mut app = App::new();
        app.generated_text = text.to_string();
        app.cursor_pos = 0;
        app.error_positions.clear();
        app.first_attempt_correct.clear();
        app.recovered_positions.clear();
        app.ever_error_positions.clear();
        app.lesson_start = Some(Instant::now());
        app.key_target_start = Some(Instant::now());
        app
    }

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
        // Type 'a' correctly
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('a'))));
        assert!(app.first_attempt_correct.contains(&0));
        assert!(!app.recovered_positions.contains(&0));
    }

    #[test]
    fn error_then_backspace_then_correct_marks_recovered() {
        let mut app = make_test_app("abc");
        app.error_mode = ErrorMode::ForgiveMistakes;
        // Type wrong char at position 0 — ForgiveMistakes advances cursor to 1
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('x'))));
        assert_eq!(app.cursor_pos, 1);
        assert!(app.error_positions.contains(&0));
        assert!(app.ever_error_positions.contains(&0));
        // Backspace — goes back to position 0, clears error mark
        update(&mut app, AppEvent::Key(make_key(KeyCode::Backspace)));
        assert_eq!(app.cursor_pos, 0);
        assert!(!app.error_positions.contains(&0));
        // Now type the correct char — should be marked as recovered
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
}
