use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::{App, AppScreen};
use crate::components::{key_bar, menu, progress, settings, stats_bar, summary, typing_area};

pub fn view(app: &App, frame: &mut Frame) {
    match app.screen {
        AppScreen::Menu => menu::render(app, frame, frame.area()),
        AppScreen::Typing => render_typing_screen(app, frame),
        AppScreen::LessonSummary => {
            render_typing_screen(app, frame);
            summary::render(app, frame, frame.area());
        }
        AppScreen::Progress => progress::render(app, frame, frame.area()),
        AppScreen::Settings => settings::render(app, frame, frame.area()),
    }
}

fn render_typing_screen(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // key bar (top)
            Constraint::Min(5),    // typing area (middle)
            Constraint::Length(2), // stats bar (bottom)
        ])
        .split(area);

    key_bar::render(app, frame, chunks[0]);
    typing_area::render(app, frame, chunks[1]);
    stats_bar::render(app, frame, chunks[2]);
}
