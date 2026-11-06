//! Progress screen key handling: back to the menu, and scrolling the
//! letter table.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::screens::progress;
use crate::app::{App, AppScreen};

/// Esc returns to the menu; the other keys scroll a letter table taller
/// than the terminal. Every step starts from the offset clamped to what
/// this terminal shows, so the first Up after End, or after a resize that
/// grew the table, already moves the view.
pub(super) fn handle_progress_key(app: &mut App, key: KeyEvent) {
    use KeyCode::*;

    let (page, max) = progress::scroll_limits(app);
    let offset = app.progress_scroll.min(max);
    match key.code {
        Esc => app.screen = AppScreen::Menu,
        Up => app.progress_scroll = offset.saturating_sub(1),
        Down => app.progress_scroll = (offset + 1).min(max),
        PageUp => app.progress_scroll = offset.saturating_sub(page),
        PageDown => app.progress_scroll = (offset + page).min(max),
        Home => app.progress_scroll = 0,
        End => app.progress_scroll = max,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ErrorMode;
    use crate::engine::Language;
    use crate::events::AppEvent;
    use crate::update::test_support::make_key;
    use crate::update::update;

    #[test]
    fn progress_esc_goes_to_menu() {
        let mut app = App::new();
        app.screen = AppScreen::Progress;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
    }

    // --- Progress screen scrolling ---

    /// German's 30 letters on the Progress screen of a terminal `height`
    /// rows tall. At 30 rows, 2 title and 3 footer rows leave a 25-row
    /// table; the header and rule take 2 and the "more" line 1, so 22
    /// letters show and the last offset is 30 - 22 = 8. At 20 rows, 12
    /// show and the last offset is 18.
    fn app_on_long_progress(height: u16) -> App {
        let mut app =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::German);
        assert_eq!(app.language.unlock_order().len(), 30);
        app.screen = AppScreen::Progress;
        app.terminal_height = height;
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        update(app, AppEvent::Key(make_key(code)));
    }

    #[test]
    fn progress_arrows_scroll_one_row() {
        let mut app = app_on_long_progress(30);
        assert_eq!(app.progress_scroll, 0);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.progress_scroll, 2);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.progress_scroll, 1);
        assert_eq!(app.screen, AppScreen::Progress);
    }

    #[test]
    fn progress_scroll_stops_at_both_ends() {
        let mut app = app_on_long_progress(30);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.progress_scroll, 0);

        press(&mut app, KeyCode::End);
        assert_eq!(app.progress_scroll, 8);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.progress_scroll, 8);

        press(&mut app, KeyCode::Home);
        assert_eq!(app.progress_scroll, 0);
    }

    #[test]
    fn progress_page_keys_move_a_page() {
        let mut app = app_on_long_progress(20);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.progress_scroll, 12);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.progress_scroll, 18, "clamped at the last letter");
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.progress_scroll, 6);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.progress_scroll, 0);
    }

    /// No dead presses at the bottom: the first Up after End moves, even
    /// when a resize to a taller terminal has since lowered the limit.
    #[test]
    fn progress_up_after_end_moves_at_once() {
        let mut app = app_on_long_progress(30);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.progress_scroll, 7);

        let mut app = app_on_long_progress(20);
        press(&mut app, KeyCode::End);
        assert_eq!(app.progress_scroll, 18);
        update(&mut app, AppEvent::Resize(30));
        press(&mut app, KeyCode::Up);
        assert_eq!(app.progress_scroll, 7);
    }

    #[test]
    fn progress_list_that_fits_does_not_scroll() {
        let mut app = App::new();
        app.screen = AppScreen::Progress;
        app.terminal_height = 40;
        for code in [KeyCode::Down, KeyCode::PageDown, KeyCode::End] {
            press(&mut app, code);
            assert_eq!(app.progress_scroll, 0, "{code:?}");
        }
    }

    #[test]
    fn opening_progress_starts_at_the_top() {
        let mut app = app_on_long_progress(30);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, AppScreen::Menu);

        app.menu_selection = 1;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, AppScreen::Progress);
        assert_eq!(app.progress_scroll, 0);
    }
}
