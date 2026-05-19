use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, AppScreen, LessonResult};
use crate::components::{key_bar, menu, progress, settings, stats_bar, typing_area};

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

    // New layout (top → bottom):
    //   Row A: 2-row progress panel (last/live stats + daily-goal bar, with
    //          an optional "+letter unlocked!" callout on line 2)
    //   Row B: 1-row characters heatmap (key_bar)
    //   Row C: typing area, vertically centered inside the remaining space
    //   Row D: 1-row footer with key hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Row A: progress panel
            Constraint::Length(1), // Row B: key bar heatmap
            Constraint::Min(0),    // Row C: typing area (centered internally)
            Constraint::Length(1), // Row D: footer
        ])
        .split(area);

    render_progress_panel(app, frame, chunks[0]);
    key_bar::render(app, frame, chunks[1]);
    typing_area::render(app, frame, chunks[2]);
    render_footer(frame, chunks[3]);
}

/// Row A — the inline progress panel.
///
/// Line 1: `Lesson N · NN wpm · NN% acc · Today: NN/NN min ░░░░░░░░░░`
/// Line 2: blank, OR a green "+letter unlocked!" callout when a new
///         letter was unlocked at the end of the previous lesson.
///
/// Left-aligned with a 28% indent so it lines up with the brand's
/// preferred horizontal rhythm on the typing screen.
fn render_progress_panel(app: &App, frame: &mut Frame, area: Rect) {
    // Two single-row strips so the optional unlock callout sits cleanly
    // below the main stats line.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // 28% indent + content area on the right.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Min(0)])
        .split(rows[0]);

    // Prefer last-lesson stats if available; otherwise show the live values
    // so the user sees something meaningful on the very first lesson too.
    // `lesson_count` is incremented inside `finish_lesson()`, so once we
    // have a `last_lesson` it points at *that* lesson's number; before then
    // we show the current in-progress lesson (lesson_count + 1).
    let (wpm_value, acc_value, lesson_n): (f64, f64, u32) = match &app.last_lesson {
        Some(LessonResult { wpm, accuracy, .. }) => (*wpm, *accuracy, app.lesson_count),
        None => (app.lesson_wpm(), app.lesson_accuracy(), app.lesson_count + 1),
    };

    let dim = Style::default().fg(Color::DarkGray);
    let sep = Span::styled(" · ", dim);

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(format!("Lesson {}", lesson_n), dim));
    spans.push(sep.clone());
    spans.push(Span::styled(format!("{:.0} wpm", wpm_value), dim));
    spans.push(sep.clone());
    spans.push(Span::styled(format!("{:.0}% acc", acc_value), dim));

    if app.daily_goal_minutes > 0 {
        let practiced_secs = app.today_seconds_practiced;
        let goal_min = app.daily_goal_minutes;
        let goal_secs = goal_min.saturating_mul(60);
        let display_minutes = (practiced_secs / 60).min(goal_min);

        spans.push(sep.clone());
        spans.push(Span::styled(
            format!("Today: {}/{} min ", display_minutes, goal_min),
            dim,
        ));
        spans.extend(stats_bar::goal_bar_spans(practiced_secs, goal_secs));
    }

    let top = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    frame.render_widget(top, cols[1]);

    // Second line: unlock callout or blank.
    let unlocked = app
        .last_lesson
        .as_ref()
        .and_then(|r| r.newly_unlocked);
    if let Some(ch) = unlocked {
        let cols2 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(28), Constraint::Min(0)])
            .split(rows[1]);
        let line = Paragraph::new(Line::from(Span::styled(
            format!("+ '{}' unlocked!", ch),
            Style::default().fg(Color::Green),
        )))
        .alignment(Alignment::Left);
        frame.render_widget(line, cols2[1]);
    }
}

/// Row D — single-line footer with key hints, centered.
fn render_footer(frame: &mut Frame, area: Rect) {
    let text = "[Esc] menu · [Tab] mode · [Ctrl+C] quit";
    let para = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(para, area);
}
