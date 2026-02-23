use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, ErrorMode};
use crate::events::AppEvent;

pub fn update(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Key(key) => handle_key(app, key),
        AppEvent::Tick => {
            // Nothing to do on tick — WPM and accuracy are computed on render.
        }
        AppEvent::Resize(_, _) => {
            // Layout recomputes automatically from the new terminal size.
            // Cursor position is derived from `cursor_pos` index each frame.
        }
    }
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    match key.code {
        // Quit
        Char('q') if key.modifiers == KeyModifiers::NONE => {
            app.running = false;
        }
        Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            app.running = false;
        }

        // Toggle error mode
        Tab => {
            app.error_mode = match app.error_mode {
                ErrorMode::MoveOn => ErrorMode::StopOnError,
                ErrorMode::StopOnError => ErrorMode::MoveOn,
            };
        }

        // Adjust proficiency speed threshold
        Char('+') | Char('=') => {
            // Easier: lower the required reaction time
            app.target_speed_ms = app.target_speed_ms.saturating_sub(25).max(100);
        }
        Char('-') => {
            // Harder: raise the required reaction time
            app.target_speed_ms = (app.target_speed_ms + 25).min(2000);
        }

        // Typing input
        Char(typed) => {
            handle_typed_char(app, typed);
        }

        _ => {}
    }
}

fn handle_typed_char(app: &mut App, typed: char) {
    // Don't process if already past end of text
    if app.cursor_pos >= app.generated_text.chars().count() {
        return;
    }

    // Start session timer on first keystroke
    if app.session_start.is_none() {
        app.session_start = Some(Instant::now());
        app.key_target_start = Some(Instant::now());
    }

    let target = match app.generated_text.chars().nth(app.cursor_pos) {
        Some(c) => c,
        None => return,
    };

    // Skip spaces automatically — they're structural, not typed characters.
    // The cursor advances through spaces without user input.
    // (We handle the space character as a separator, not a key to type.)
    // Actually, spaces are included as typed characters for realism.

    let reaction_ms = app
        .key_target_start
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);

    if typed == target {
        // Correct keystroke
        let stats = app.per_key_stats.entry(target).or_default();
        // Only record reaction for actual letters (not spaces)
        if target != ' ' {
            stats.record_hit(reaction_ms);
        }
        app.correct_chars += 1;
        app.cursor_pos += 1;
        app.key_target_start = Some(Instant::now());

        // Skip any spaces automatically so the next target is always a letter
        while app.cursor_pos < app.generated_text.chars().count() {
            if app.generated_text.chars().nth(app.cursor_pos) == Some(' ') {
                app.cursor_pos += 1;
                app.correct_chars += 1;
            } else {
                break;
            }
        }

        // Check if we've finished the current batch
        if app.cursor_pos >= app.generated_text.chars().count() {
            app.advance_batch();
        }
    } else {
        // Wrong keystroke
        let stats = app.per_key_stats.entry(target).or_default();
        if target != ' ' {
            stats.record_error();
        }

        match app.error_mode {
            ErrorMode::MoveOn => {
                // Mark the position as an error and advance
                app.error_positions.insert(app.cursor_pos);
                app.cursor_pos += 1;
                app.key_target_start = Some(Instant::now());

                // Check if finished
                if app.cursor_pos >= app.generated_text.chars().count() {
                    app.advance_batch();
                }
            }
            ErrorMode::StopOnError => {
                // Cursor stays; just record the error (don't double-count)
                app.error_positions.insert(app.cursor_pos);
                // Reset the reaction timer so we don't accumulate huge times
                app.key_target_start = Some(Instant::now());
            }
        }
    }
}
