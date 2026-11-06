//! Menu screen key handling.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::request_stats_save;
use crate::app::{App, AppScreen};
use crate::screens::menu::MENU_ITEMS;

pub(super) fn handle_menu_key(app: &mut App, key: KeyEvent) {
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
                    // View Progress, from the top of the table
                    app.progress_scroll = 0;
                    app.screen = AppScreen::Progress;
                }
                2 => {
                    // Settings
                    app.screen = AppScreen::Settings;
                }
                3 => {
                    // Quit
                    request_stats_save(app);
                    app.running = false;
                }
                _ => {}
            }
        }
        Char('q') | Esc => {
            request_stats_save(app);
            app.running = false;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AppEvent;
    use crate::update::test_support::make_key;
    use crate::update::update;

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
    fn quit_raises_stats_save_flag() {
        let mut app = App::new();
        assert!(!app.pending_stats_save);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert!(!app.running);
        assert!(app.pending_stats_save, "quit must request a stats save");
    }
}
