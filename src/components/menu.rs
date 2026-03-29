use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use crate::app::App;

pub const MENU_ITEMS: &[&str] = &["Start Practice", "View Progress", "Settings", "Quit"];

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(10),
            Constraint::Percentage(30),
        ])
        .split(area);

    let inner = centered_rect(v_chunks[1], 50);

    let mut lines: Vec<Line> = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        "keybr-tui",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Menu items
    for (i, &item) in MENU_ITEMS.iter().enumerate() {
        let is_selected = i == app.menu_selection;
        if is_selected {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  > {}", item),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!("    {}", item),
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Up/Down] navigate  [Enter] select  [q] quit",
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
