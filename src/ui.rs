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
    //   Row A: 5-row dashboard (Metrics / All keys / Current / Daily / callout)
    //   Row B: blank spacer so the dashboard doesn't crowd the typing area
    //   Row C: typing area, vertically centered inside the remaining space
    //   Row D: 1-row footer with key hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Row A: dashboard
            Constraint::Length(1), // Row B: spacer
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
