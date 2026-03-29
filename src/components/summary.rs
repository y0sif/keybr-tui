use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(8),
            Constraint::Percentage(30),
        ])
        .split(area);

    let inner = centered_rect(v_chunks[1], 50);

    // Clear the area behind the modal
    frame.render_widget(Clear, inner);

    let result = match &app.last_lesson {
        Some(r) => r,
        None => return,
    };

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "-- lesson complete --",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled("  wpm      ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}", result.wpm),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  accuracy ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}%", result.accuracy),
            accuracy_style(result.accuracy),
        ),
    ]));

    if let Some(new_key) = result.newly_unlocked {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  unlocked ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                new_key.to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if !result.weakest_keys.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  weakest keys",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        for &(key, conf) in &result.weakest_keys {
            let conf_style = if conf >= 1.0 {
                Style::default().fg(Color::Green)
            } else if conf >= 0.5 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Red)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("    {} ", key), Style::default().fg(Color::White)),
                Span::styled(format!("{:.0}%", conf * 100.0), conf_style),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  any key: next lesson | Esc: menu",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Left);

    frame.render_widget(para, inner);
}

fn accuracy_style(acc: f64) -> Style {
    if acc >= 95.0 {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else if acc >= 80.0 {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
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
