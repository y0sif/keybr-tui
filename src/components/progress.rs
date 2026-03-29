use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::engine::scheduler::UNLOCK_ORDER;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(Span::styled(
        "Key Progress",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(title, v_chunks[0]);

    // Table content
    let inner = centered_rect(v_chunks[1], 80);
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();

    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![Span::styled(
        "  Key   Confidence   Speed(ms)   Errors       Status",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(Span::styled(
        "  ---   ----------   ---------   ------       ------",
        Style::default().fg(Color::DarkGray),
    )));

    for &key in UNLOCK_ORDER {
        let is_active = active_set.contains(&key);
        let is_focused = app.scheduler.focused_key == Some(key);

        if let Some(stats) = app.per_key_stats.get(&key) {
            let conf = stats.confidence(app.target_cpm);
            let conf_pct = (conf * 100.0).round() as i32;
            let speed = if stats.filtered_time_ms > 0.0 {
                format!("{:.0}", stats.filtered_time_ms)
            } else {
                "----".to_string()
            };
            let errors = format!("{}/{}", stats.errors, stats.attempts);

            let status = if !is_active {
                "Locked"
            } else if conf >= 1.0 {
                "Learned"
            } else {
                "Active"
            };

            let focus_marker = if is_focused { " < Focus" } else { "" };

            let row = format!(
                "   {}     {:>4}%        {:>5}       {:>8}     {}{}",
                key, conf_pct, speed, errors, status, focus_marker
            );

            let color = if !is_active {
                Color::DarkGray
            } else if conf >= 1.0 {
                Color::Green
            } else {
                Color::White
            };

            let mut style = Style::default().fg(color);
            if is_focused {
                style = style.add_modifier(Modifier::BOLD);
            }

            lines.push(Line::from(Span::styled(row, style)));
        } else {
            // No stats yet
            let status = if is_active { "Active" } else { "Locked" };
            let focus_marker = if is_focused { " < Focus" } else { "" };
            let row = format!(
                "   {}     ----        -----       --------     {}{}",
                key, status, focus_marker
            );
            let color = if is_active {
                Color::White
            } else {
                Color::DarkGray
            };
            let mut style = Style::default().fg(color);
            if is_focused {
                style = style.add_modifier(Modifier::BOLD);
            }
            lines.push(Line::from(Span::styled(row, style)));
        }
    }

    let table = Paragraph::new(Text::from(lines)).alignment(Alignment::Left);
    frame.render_widget(table, inner);

    // Footer
    let footer_text = format!(
        "  Lessons completed: {}    [Esc] Back to menu",
        app.lesson_count
    );
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(footer, v_chunks[2]);
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
