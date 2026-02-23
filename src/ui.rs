use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, ErrorMode};

pub fn view(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Outer layout: stats bar (top), typing area (middle), key bar (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // stats bar
            Constraint::Min(5),    // typing area
            Constraint::Length(2), // key unlock bar
        ])
        .split(area);

    render_stats(app, frame, chunks[0]);
    render_typing_area(app, frame, chunks[1]);
    render_key_bar(app, frame, chunks[2]);
}

// ─── Stats bar ───────────────────────────────────────────────────────────────

fn render_stats(app: &App, frame: &mut Frame, area: Rect) {
    let wpm = app.wpm();
    let acc = app.accuracy();

    let mode_label = match app.error_mode {
        ErrorMode::MoveOn => "move-on",
        ErrorMode::StopOnError => "stop",
    };

    let stats_text = format!(
        " {:.0} wpm   {:.0}% acc   goal: {}ms [±25 -/+]   mode: {} [tab]   q: quit",
        wpm, acc, app.target_speed_ms, mode_label
    );

    let para = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Left);

    frame.render_widget(para, area);
}

// ─── Typing area ─────────────────────────────────────────────────────────────

fn render_typing_area(app: &App, frame: &mut Frame, area: Rect) {
    let spans = build_spans(app);
    let line = Line::from(spans);
    let text = Text::from(line);

    let para = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .style(Style::default()),
        );

    // Center the paragraph vertically within the typing area
    let inner = centered_rect(area, 80);
    frame.render_widget(para, inner);
}

/// Build the list of styled spans for the generated text.
fn build_spans(app: &App) -> Vec<Span<'static>> {
    let text_chars: Vec<char> = app.generated_text.chars().collect();
    let mut spans = Vec::with_capacity(text_chars.len());

    for (i, &ch) in text_chars.iter().enumerate() {
        let s = ch.to_string();
        let span = if i < app.cursor_pos {
            if app.error_positions.contains(&i) {
                // Incorrectly typed
                Span::styled(s, Style::default().fg(Color::Red))
            } else {
                // Correctly typed
                Span::styled(s, Style::default().fg(Color::Green))
            }
        } else if i == app.cursor_pos {
            // Current target — block cursor via color inversion
            Span::styled(s, Style::default().add_modifier(Modifier::REVERSED))
        } else {
            // Upcoming / untyped
            Span::styled(s, Style::default().fg(Color::DarkGray))
        };
        spans.push(span);
    }

    spans
}

// ─── Key bar ─────────────────────────────────────────────────────────────────

fn render_key_bar(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" keys: ", Style::default().fg(Color::DarkGray)));

    for (i, &key) in app.scheduler.active_keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", Style::default()));
        }

        let stats = app.per_key_stats.get(&key);
        let is_proficient = stats
            .map(|s| s.is_proficient(app.target_speed_ms))
            .unwrap_or(false);

        let style = if is_proficient {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        spans.push(Span::styled(key.to_string(), style));
    }

    // Show hint for next unlock if not all unlocked
    if !app.scheduler.all_unlocked() {
        spans.push(Span::styled(
            " (master all to unlock next)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let para = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    frame.render_widget(para, area);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Return a centered sub-rect of the given area, with a max width percentage.
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
