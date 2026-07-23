use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, ErrorMode};

/// Settings items the user can navigate between.
pub const SETTINGS_COUNT: usize = 5;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(10),
            Constraint::Percentage(30),
        ])
        .split(area);

    let inner = centered_rect(v_chunks[1], 60);

    let mut lines: Vec<Line> = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        "Settings",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Target WPM
    let wpm_style = if app.settings_selection == 0 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let wpm_marker = if app.settings_selection == 0 {
        "> "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{}Target WPM         ", wpm_marker), wpm_style),
        Span::styled(format!("[  {}  ]", app.target_wpm()), wpm_style),
        Span::styled(
            "     Left/Right to adjust",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Error Mode
    let mode_style = if app.settings_selection == 1 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mode_marker = if app.settings_selection == 1 {
        "> "
    } else {
        "  "
    };
    let mode_label = match app.error_mode {
        ErrorMode::ForgiveMistakes => "Forgive Mistakes",
        ErrorMode::StopOnError => "Stop On Error",
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{}Error Mode         ", mode_marker), mode_style),
        Span::styled(format!("[  {}  ]", mode_label), mode_style),
        Span::styled(
            "     Left/Right to toggle",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Fragment Length
    let frag_style = if app.settings_selection == 2 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let frag_marker = if app.settings_selection == 2 {
        "> "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{}Fragment Length     ", frag_marker), frag_style),
        Span::styled(format!("[  {}  ]", app.fragment_length), frag_style),
        Span::styled(
            "     Left/Right to adjust",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Alphabet Size (extra letters force-included beyond the 6 starters)
    let alpha_style = if app.settings_selection == 3 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let alpha_marker = if app.settings_selection == 3 {
        "> "
    } else {
        "  "
    };
    let extra = crate::engine::scheduler::forced_extra_letters(app.alphabet_size);
    lines.push(Line::from(vec![
        Span::styled(format!("{}Alphabet size       ", alpha_marker), alpha_style),
        Span::styled(format!("[  +{extra} letters  ]"), alpha_style),
        Span::styled(
            "     Left/Right to adjust",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Focus letter (manual pin overriding the scheduler's auto pick)
    let focus_style = if app.settings_selection == 4 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let focus_marker = if app.settings_selection == 4 {
        "> "
    } else {
        "  "
    };
    let focus_label = match app.manual_focus {
        Some(c) => c.to_ascii_uppercase().to_string(),
        None => "Auto".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{}Focus letter        ", focus_marker), focus_style),
        Span::styled(format!("[  {focus_label}  ]"), focus_style),
        Span::styled(
            "     Left/Right to adjust",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Esc] Back to menu",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(para, inner);
}

fn centered_rect(area: Rect, max_width_pct: u16) -> Rect {
    let h_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - max_width_pct) / 2),
            Constraint::Percentage(max_width_pct),
            Constraint::Percentage((100 - max_width_pct) / 2),
        ])
        .split(area);
    h_split[1]
}
