//! Screen structure shared by `update` and the views: the menu entries,
//! the settings rows and the progress table's geometry. It depends on no
//! `App` state, so the model never holds view geometry. Nothing here
//! renders.

/// The main menu.
pub mod menu {
    pub const MENU_ITEMS: &[&str] = &["Start Practice", "View Progress", "Settings", "Quit"];
}

/// The settings screen.
pub mod settings {
    /// Settings items the user can navigate between.
    pub const SETTINGS_COUNT: usize = 7;

    /// Index of the Language row. Last on purpose: `settings_selection`
    /// starts at 0 at launch, so the first visit to Settings never opens on
    /// the Language row. Later visits return to the row last selected.
    pub const LANGUAGE_ROW: usize = 6;
}

/// The progress screen's letter table.
pub mod progress {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};

    /// Rows the header and its rule take at the top of the table.
    const HEADER_ROWS: u16 = 2;

    /// Title, table and footer. Shared by `render` and `scroll_limits`, so the
    /// scroll clamp in `update` measures the same table the frame draws.
    pub(crate) fn screen_layout(area: Rect) -> [Rect; 3] {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .areas(area)
    }

    /// How many letter rows a table `height` rows tall shows at once, or
    /// `None` when all `letters` fit under the header. A list that does not
    /// fit gives up one more row, to the "more" line below the letters.
    pub(crate) fn scrolling_rows(height: u16, letters: usize) -> Option<usize> {
        let space = usize::from(height.saturating_sub(HEADER_ROWS));
        (letters > space).then(|| space.saturating_sub(1).max(1))
    }

    /// The scroll limits of a table of `letters` letters on a terminal
    /// `height` rows tall, as `(page, max)`: how far PageUp/PageDown move,
    /// and the offset that brings the last letter into view. A list that
    /// fits has a `max` of 0.
    ///
    /// `update` has no frame to measure, so it passes the terminal height
    /// it last saw; `render` measures the frame it is given.
    pub(crate) fn scroll_limits(letters: usize, height: u16) -> (usize, usize) {
        let [_, table, _] = screen_layout(Rect::new(0, 0, 1, height));
        match scrolling_rows(table.height, letters) {
            Some(rows) => (rows, letters - rows),
            None => (letters, 0),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::engine::Language;

        #[test]
        fn scroll_limits_follow_the_terminal_height() {
            let english = Language::English.unlock_order().len();
            // 33 rows hold English whole: title 2, header and rule 2, 26
            // letters, footer 3.
            assert_eq!(scroll_limits(english, 33), (26, 0));
            // One row fewer and the "more" line takes another: 24 letters show.
            assert_eq!(scroll_limits(english, 32), (24, 2));
            let letters = Language::German.unlock_order().len();
            assert_eq!(scroll_limits(letters, 30), (22, letters - 22));
            // However small the terminal, a page is a row or more and the last
            // offset still brings the last letter into view.
            for height in 0..15 {
                let (page, max) = scroll_limits(letters, height);
                assert!(page >= 1, "height {height}");
                assert_eq!(page + max, letters, "height {height}");
            }
        }
    }
}
