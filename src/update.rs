use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppScreen, ErrorMode};
use crate::components::menu::MENU_ITEMS;
use crate::components::settings::SETTINGS_COUNT;
use crate::events::AppEvent;
use crate::persistence::today_date_string;

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

pub fn update(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Key(key) => handle_key(app, key),
        AppEvent::Tick => {}
        AppEvent::Resize => {}
    }
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    // Global: Ctrl+C always quits
    if key.code == Char('c') && key.modifiers == KeyModifiers::CONTROL {
        auto_save_stats(app);
        app.running = false;
        return;
    }

    match app.screen {
        AppScreen::Menu => handle_menu_key(app, key),
        AppScreen::Typing => handle_typing_key(app, key),
        AppScreen::Progress => handle_progress_key(app, key),
        AppScreen::Settings => handle_settings_key(app, key),
    }
}

// --- Menu screen ---

fn handle_menu_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    match key.code {
        Up => {
            if app.menu_selection > 0 {
                app.menu_selection -= 1;
            } else {
                app.menu_selection = MENU_ITEMS.len() - 1;
            }
        }
        Down => {
            app.menu_selection = (app.menu_selection + 1) % MENU_ITEMS.len();
        }
        Enter => {
            match app.menu_selection {
                0 => {
                    // Start Practice
                    app.start_next_lesson();
                }
                1 => {
                    // View Progress
                    app.screen = AppScreen::Progress;
                }
                2 => {
                    // Settings
                    app.screen = AppScreen::Settings;
                }
                3 => {
                    // Quit
                    auto_save_stats(app);
                    app.running = false;
                }
                _ => {}
            }
        }
        Char('q') | Esc => {
            auto_save_stats(app);
            app.running = false;
        }
        _ => {}
    }
}

// --- Typing screen ---

fn handle_typing_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    match key.code {
        Esc => {
            // Esc during typing goes to menu, not quit
            auto_save_stats(app);
            app.screen = AppScreen::Menu;
        }

        // Toggle error mode
        Tab => {
            app.error_mode = match app.error_mode {
                ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
            };
            auto_save_config(app);
        }

        // Backspace — move cursor back and clear error mark
        Backspace => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.error_positions.remove(&app.cursor_pos);
                app.first_attempt_correct.remove(&app.cursor_pos);
                app.recovered_positions.remove(&app.cursor_pos);
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
        let pos = app.cursor_pos;

        if app.ever_error_positions.contains(&pos) {
            app.recovered_positions.insert(pos);
        } else {
            app.first_attempt_correct.insert(pos);
        }

        if target != ' ' {
            if app.first_attempt_correct.contains(&pos) {
                let stats = app.per_key_stats.entry(target).or_default();
                if (40..=12000).contains(&reaction_ms) {
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
            auto_save_stats(app);
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
                    auto_save_stats(app);
                }
            }
            ErrorMode::StopOnError => {
                app.error_positions.insert(pos);
                app.key_target_start = Some(Instant::now());
            }
        }
    }
}

// --- Progress screen ---

fn handle_progress_key(app: &mut App, key: crossterm::event::KeyEvent) {
    if key.code == KeyCode::Esc {
        app.screen = AppScreen::Menu;
    }
}

// --- Settings screen ---

fn handle_settings_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    match key.code {
        Esc => {
            auto_save_config(app);
            app.screen = AppScreen::Menu;
        }
        Up => {
            if app.settings_selection > 0 {
                app.settings_selection -= 1;
            } else {
                app.settings_selection = SETTINGS_COUNT - 1;
            }
        }
        Down => {
            app.settings_selection = (app.settings_selection + 1) % SETTINGS_COUNT;
        }
        Left => {
            match app.settings_selection {
                0 => {
                    // Decrease target WPM
                    let new_wpm = app.target_wpm().saturating_sub(5).max(10);
                    app.set_target_wpm(new_wpm);
                }
                1 => {
                    // Toggle error mode
                    app.error_mode = match app.error_mode {
                        ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                        ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                    };
                }
                2 => {
                    // Decrease fragment length
                    app.fragment_length = app.fragment_length.saturating_sub(10).max(20);
                }
                _ => {}
            }
            auto_save_config(app);
        }
        Right => {
            match app.settings_selection {
                0 => {
                    // Increase target WPM
                    let new_wpm = (app.target_wpm() + 5).min(200);
                    app.set_target_wpm(new_wpm);
                }
                1 => {
                    // Toggle error mode
                    app.error_mode = match app.error_mode {
                        ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                        ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                    };
                }
                2 => {
                    // Increase fragment length
                    app.fragment_length = (app.fragment_length + 10).min(500);
                }
                _ => {}
            }
            auto_save_config(app);
        }
        Enter => {
            // Toggle error mode on Enter when selected
            if app.settings_selection == 1 {
                app.error_mode = match app.error_mode {
                    ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                    ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                };
                auto_save_config(app);
            }
        }
        _ => {}
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
    fn esc_on_menu_quits() {
        let mut app = App::new();
        assert_eq!(app.screen, AppScreen::Menu);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert!(!app.running);
    }

    #[test]
    fn q_on_menu_quits() {
        let mut app = App::new();
        assert_eq!(app.screen, AppScreen::Menu);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Char('q'))));
        assert!(!app.running);
    }

    #[test]
    fn menu_navigation() {
        let mut app = App::new();
        assert_eq!(app.menu_selection, 0);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Down)));
        assert_eq!(app.menu_selection, 1);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Down)));
        assert_eq!(app.menu_selection, 2);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Up)));
        assert_eq!(app.menu_selection, 1);
    }

    #[test]
    fn menu_wraps_around() {
        let mut app = App::new();
        assert_eq!(app.menu_selection, 0);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Up)));
        assert_eq!(app.menu_selection, MENU_ITEMS.len() - 1);
    }

    #[test]
    fn menu_enter_starts_practice() {
        let mut app = App::new();
        app.menu_selection = 0;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Enter)));
        assert_eq!(app.screen, AppScreen::Typing);
    }

    #[test]
    fn menu_enter_opens_progress() {
        let mut app = App::new();
        app.menu_selection = 1;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Enter)));
        assert_eq!(app.screen, AppScreen::Progress);
    }

    #[test]
    fn menu_enter_opens_settings() {
        let mut app = App::new();
        app.menu_selection = 2;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Enter)));
        assert_eq!(app.screen, AppScreen::Settings);
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

    #[test]
    fn progress_esc_goes_to_menu() {
        let mut app = App::new();
        app.screen = AppScreen::Progress;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
    }

    #[test]
    fn settings_esc_goes_to_menu() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
    }

    #[test]
    fn settings_adjusts_wpm() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 0;
        let initial_wpm = app.target_wpm();
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.target_wpm(), initial_wpm + 5);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.target_wpm(), initial_wpm);
    }

    #[test]
    fn settings_toggles_error_mode() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 1;
        app.error_mode = ErrorMode::ForgiveMistakes;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.error_mode, ErrorMode::StopOnError);
    }

    #[test]
    fn settings_adjusts_fragment_length() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 2;
        let initial = app.fragment_length;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.fragment_length, initial + 10);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.fragment_length, initial);
    }
}
