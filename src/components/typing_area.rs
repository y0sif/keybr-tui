use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let spans = build_spans(app);
    let line = Line::from(spans);
    let text = Text::from(line);

    let para = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::NONE));

    // Center vertically inside whatever the parent layout gave us, then
    // narrow horizontally to ~80% of that strip.
    let v_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Min(0),
            Constraint::Percentage(33),
        ])
        .split(area);
    let inner = centered_rect(v_split[1], 80);
    frame.render_widget(para, inner);
}

/// Build the styled span list for the current generated text.
fn build_spans(app: &App) -> Vec<Span<'static>> {
    let text_chars: Vec<char> = app.generated_text.chars().collect();
    let mut spans = Vec::with_capacity(text_chars.len());

    for (i, &ch) in text_chars.iter().enumerate() {
        let s = ch.to_string();
        let span = if i < app.cursor_pos {
            if app.error_positions.contains(&i) {
                Span::styled(s, Style::default().fg(Color::Red))
            } else if app.recovered_positions.contains(&i) {
                Span::styled(s, Style::default().fg(Color::Yellow))
            } else {
                Span::styled(s, Style::default().fg(Color::Green))
            }
        } else if i == app.cursor_pos {
            Span::styled(s, Style::default().add_modifier(Modifier::REVERSED))
        } else {
            Span::styled(s, Style::default().fg(Color::DarkGray))
        };
        spans.push(span);
    }

    spans
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
