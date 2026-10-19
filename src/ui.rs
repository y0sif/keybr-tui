use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, AppScreen, ErrorMode};
use crate::components::{dashboard, menu, progress, settings, typing_area};

pub fn view(app: &App, frame: &mut Frame) {
    match app.screen {
        AppScreen::Menu => menu::render(app, frame, frame.area()),
        AppScreen::Typing => render_typing_screen(app, frame),
        AppScreen::Progress => progress::render(app, frame, frame.area()),
        AppScreen::Settings => settings::render(app, frame, frame.area()),
    }
}

fn render_typing_screen(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Top-to-bottom layout:
    //   Row A: dashboard (Metrics / All keys / Current / Daily / callout):
    //          5 rows, more when the heatmap wraps on a narrow terminal
    //   Row B: blank spacer so the dashboard doesn't crowd the typing area
    //   Row C: typing area, vertically centered inside the remaining space
    //   Row D: 1-row footer with key hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(dashboard::height(app, area.width)), // Row A: dashboard
            Constraint::Length(1),                                  // Row B: spacer
            Constraint::Min(0),    // Row C: typing area (centered internally)
            Constraint::Length(1), // Row D: footer
        ])
        .split(area);

    dashboard::render(app, frame, chunks[0]);
    typing_area::render(app, frame, chunks[2]);
    render_footer(app, frame, chunks[3]);
}

/// Row D — single-line footer with key hints, centered.
///
/// The mode segment makes the current error mode visible at a glance and
/// shows what [Tab] will switch to next, so users always know which mode
/// they're in without trial-and-error. Lesson count moved here from the
/// dashboard so the dashboard rows stay focused on per-lesson stats.
fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let active = Style::default().fg(Color::White);

    let (current_label, next_label) = match app.error_mode {
        ErrorMode::ForgiveMistakes => ("Forgive", "Stop"),
        ErrorMode::StopOnError => ("Stop", "Forgive"),
    };

    let spans = vec![
        Span::styled(format!("Lesson {}  ·  ", app.lesson_count + 1), dim),
        Span::styled("[Esc] menu  ·  Mode: ", dim),
        Span::styled(current_label, active),
        Span::styled(format!("  [Tab → {}]  ·  [Ctrl+C] quit", next_label), dim),
    ];

    let para = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    /// Render a fresh app's typing screen into a `width` x 30 buffer.
    fn render_typing(language: Language, width: u16) -> (App, Buffer) {
        let mut app = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, language);
        app.screen = AppScreen::Typing;
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).expect("terminal");
        terminal.draw(|frame| view(&app, frame)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (app, buffer)
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// The letters drawn on tiles (cells with a background color) in row
    /// `y`, left to right.
    fn tile_letters(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| &buffer[(x, y)])
            .filter(|cell| cell.bg != Color::Reset && cell.symbol() != " ")
            .map(|cell| cell.symbol())
            .collect()
    }

    fn all_tiles(language: Language) -> String {
        language
            .letters()
            .iter()
            .map(|&c| language.upper(c))
            .collect()
    }

    /// At 80 columns no language's heatmap fits on one row: every letter
    /// still shows exactly once, in order, and the rows below it move
    /// down instead of being drawn over.
    #[test]
    fn narrow_terminal_wraps_the_heatmap_without_losing_letters() {
        for &language in Language::ALL {
            let (app, buffer) = render_typing(language, 80);
            let heatmap_rows = dashboard::height(&app, 80) - 4;
            assert_eq!(heatmap_rows, 2, "{language:?}");

            let shown: String = (1..1 + heatmap_rows)
                .map(|y| tile_letters(&buffer, y))
                .collect();
            assert_eq!(shown, all_tiles(language), "{language:?}");

            // Wrapped rows share one left edge, so tile columns line up.
            let first_tile_x = |y: u16| (0..80).find(|&x| buffer[(x, y)].bg != Color::Reset);
            assert_eq!(first_tile_x(1), first_tile_x(2), "{language:?}");

            // The band fills the 80 columns, so every label starts at x 0.
            assert!(row_text(&buffer, 0).starts_with("Metrics:"));
            assert!(row_text(&buffer, 1).starts_with("All keys:"));
            assert!(row_text(&buffer, 3).starts_with("Current:"), "{language:?}");
            assert!(
                row_text(&buffer, 4).starts_with("Daily goal:"),
                "{language:?}"
            );
            // Callout (blank on a fresh app) and spacer stay clear.
            assert_eq!(row_text(&buffer, 5).trim(), "", "{language:?}");
            assert_eq!(row_text(&buffer, 6).trim(), "", "{language:?}");
            assert!(row_text(&buffer, 29).contains("Lesson 1"));
        }
    }

    /// On a short terminal the settings box still holds every row, the
    /// Language row last with its language's own name, and the hint below.
    #[test]
    fn short_terminal_settings_keep_the_language_row_and_the_hint() {
        for &language in Language::ALL {
            let mut app = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, language);
            app.screen = AppScreen::Settings;
            let mut terminal = Terminal::new(TestBackend::new(120, 14)).expect("terminal");
            terminal.draw(|frame| view(&app, frame)).expect("draw");
            let buffer = terminal.backend().buffer().clone();
            let rows: Vec<String> = (0..14).map(|y| row_text(&buffer, y)).collect();

            let language_row = rows
                .iter()
                .position(|r| r.contains("Language"))
                .unwrap_or_else(|| panic!("{language:?}: no Language row in {rows:#?}"));
            assert!(
                rows[language_row].contains(language.display_name()),
                "{language:?}"
            );
            assert!(
                rows[language_row - 1].contains("Focus pattern"),
                "{language:?}"
            );
            assert!(
                rows[language_row + 1..]
                    .iter()
                    .any(|r| r.contains("[Esc] Back to menu")),
                "{language:?}: hint clipped in {rows:#?}"
            );
        }
    }

    /// Wide enough for the single row, the dashboard keeps its five rows
    /// and the heatmap sits centered on row 1, as it always has.
    #[test]
    fn wide_terminal_keeps_the_single_centered_row() {
        for &language in Language::ALL {
            let (app, buffer) = render_typing(language, 120);
            assert_eq!(dashboard::height(&app, 120), 5, "{language:?}");

            let letters = language.letters();
            let band = 12 + letters.len() as u16 * 3;
            let left = (120 - band) / 2;
            // Everything left of the band is blank, so byte offset = column.
            assert!(row_text(&buffer, 1)[left as usize..].starts_with("All keys:"));
            for (i, &c) in letters.iter().enumerate() {
                let x = left + 12 + 3 * i as u16 + 1;
                assert_eq!(buffer[(x, 1)].symbol(), language.upper(c), "{language:?}");
            }
            assert_eq!(tile_letters(&buffer, 1), all_tiles(language));
            assert!(row_text(&buffer, 2)[left as usize..].starts_with("Current:"));
        }
    }
}
