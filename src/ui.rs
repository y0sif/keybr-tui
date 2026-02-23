use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, AppScreen, ErrorMode};

/// All letters in unlock order (matches scheduler::UNLOCK_ORDER).
const UNLOCK_ORDER: &[char] = &[
    'e', 't', 'a', 'o', 'i', 'n',
    's', 'r', 'h', 'l', 'd', 'c',
    'u', 'm', 'f', 'p', 'g', 'w',
    'y', 'b', 'v', 'k', 'x', 'j',
    'q', 'z',
];

pub fn view(app: &App, frame: &mut Frame) {
    match app.screen {
        AppScreen::Typing => render_typing_screen(app, frame),
        AppScreen::LessonSummary => render_summary_screen(app, frame),
    }
}

// ─── Typing screen ───────────────────────────────────────────────────────────

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

    render_key_bar(app, frame, chunks[0]);
    render_typing_area(app, frame, chunks[1]);
    render_stats(app, frame, chunks[2]);
}

// ─── Key bar (top) ───────────────────────────────────────────────────────────

fn render_key_bar(app: &App, frame: &mut Frame, area: Rect) {
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Style::default()));

    let mut past_active = false;

    for &key in UNLOCK_ORDER {
        let is_active = active_set.contains(&key);

        // Insert separator between active and locked sections
        if !is_active && !past_active {
            past_active = true;
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        } else {
            spans.push(Span::styled(" ", Style::default()));
        }

        let style = if is_active {
            let is_proficient = app
                .per_key_stats
                .get(&key)
                .map(|s| s.is_proficient(app.target_speed_ms()))
                .unwrap_or(false);
            if is_proficient {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            }
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(key.to_string(), style));
    }

    let para = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    frame.render_widget(para, area);
}

// ─── Typing area (middle) ────────────────────────────────────────────────────

fn render_typing_area(app: &App, frame: &mut Frame, area: Rect) {
    let spans = build_spans(app);
    let line = Line::from(spans);
    let text = Text::from(line);

    let para = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::NONE));

    let inner = centered_rect(area, 80);
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

// ─── Stats bar (bottom) ──────────────────────────────────────────────────────

fn render_stats(app: &App, frame: &mut Frame, area: Rect) {
    let mode_label = match app.error_mode {
        ErrorMode::MoveOn => "move-on",
        ErrorMode::StopOnError => "stop",
    };

    let stats_text = if let Some(last) = &app.last_lesson {
        format!(
            " last lesson: {:.0} wpm  {:.0}% acc   goal: {} wpm [±5 -/+]   mode: {} [tab]   esc: quit",
            last.wpm, last.accuracy, app.target_wpm, mode_label
        )
    } else {
        format!(
            " no lesson yet   goal: {} wpm [±5 -/+]   mode: {} [tab]   esc: quit",
            app.target_wpm, mode_label
        )
    };

    let para = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Left);

    frame.render_widget(para, area);
}

// ─── Lesson summary screen ───────────────────────────────────────────────────

fn render_summary_screen(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Center a block vertically and horizontally
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(8),
            Constraint::Percentage(30),
        ])
        .split(area);

    let inner = centered_rect(v_chunks[1], 50);

    let result = match &app.last_lesson {
        Some(r) => r,
        None => return,
    };

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "── lesson complete ──",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled("  wpm      ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}", result.wpm),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
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

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  press any key to continue",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Left);

    frame.render_widget(para, inner);
}

fn accuracy_style(acc: f64) -> Style {
    if acc >= 95.0 {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else if acc >= 80.0 {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
