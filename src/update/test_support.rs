//! Test helpers shared by the per-screen test modules.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use crate::app::{App, AppScreen};

pub(super) fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(super) fn make_test_app(text: &str) -> App {
    let mut app = App::new();
    app.screen = AppScreen::Typing;
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
