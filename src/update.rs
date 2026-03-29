use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppScreen, ErrorMode};
use crate::events::AppEvent;

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
        app.running = false;
        return;
    }
    if key.code == Char('c') && key.modifiers == KeyModifiers::CONTROL {
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
                ErrorMode::MoveOn => ErrorMode::StopOnError,
                ErrorMode::StopOnError => ErrorMode::MoveOn,
            };
        }

        // Adjust WPM goal
        Char('+') | Char('=') => {
            let new_wpm = (app.target_wpm() + 5).min(200);
            app.set_target_wpm(new_wpm);
        }
        Char('-') => {
            let new_wpm = app.target_wpm().saturating_sub(5).max(10);
            app.set_target_wpm(new_wpm);
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
        // Correct keystroke — record stats for letters (not spaces)
        if target != ' ' {
            let stats = app.per_key_stats.entry(target).or_default();
            stats.record_hit(reaction_ms);
            app.lesson_positions += 1;
        }
        app.lesson_correct += 1;
        app.cursor_pos += 1;
        app.key_target_start = Some(Instant::now());

        // Check if we've finished the current lesson
        if app.cursor_pos >= app.generated_text.chars().count() {
            app.finish_lesson();
        }
    } else {
        // Wrong keystroke
        if target != ' ' {
            let stats = app.per_key_stats.entry(target).or_default();
            stats.record_error();
            // Only count first-try errors (don't double-count in StopOnError mode)
            if !app.error_positions.contains(&app.cursor_pos) {
                app.lesson_positions += 1;
                app.lesson_errors += 1;
            }
        }

        match app.error_mode {
            ErrorMode::MoveOn => {
                app.error_positions.insert(app.cursor_pos);
                app.cursor_pos += 1;
                app.key_target_start = Some(Instant::now());

                if app.cursor_pos >= app.generated_text.chars().count() {
                    app.finish_lesson();
                }
            }
            ErrorMode::StopOnError => {
                app.error_positions.insert(app.cursor_pos);
                app.key_target_start = Some(Instant::now());
            }
        }
    }
}
