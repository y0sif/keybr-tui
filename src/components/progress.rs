use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::engine::scheduler::UNLOCK_ORDER;
use crate::metrics::KeyStats;

// Column widths.
// Key (3) | WPM (5) | Best (5) | Attempts (10) | Errors (8) | Status (12)
const W_KEY: usize = 3;
const W_WPM: usize = 5;
const W_BEST: usize = 5;
const W_ATTEMPTS: usize = 10;
const W_ERRORS: usize = 8;
const W_STATUS: usize = 12;

const NO_SAMPLE: &str = "—";

/// Progress tier derived from confidence vs. target CPM.
/// `Locked` keys haven't been unlocked by the scheduler yet.
enum Tier {
    Locked,
    Early,
    Progressing,
    Learned,
}

impl Tier {
    fn label(&self) -> &'static str {
        match self {
            Tier::Locked => "Locked",
            Tier::Early => "Early",
            Tier::Progressing => "Progressing",
            Tier::Learned => "Learned",
        }
    }

    fn color(&self) -> Color {
        match self {
            Tier::Locked => Color::DarkGray,
            Tier::Early => Color::White,
            Tier::Progressing => Color::Yellow,
            Tier::Learned => Color::Green,
        }
    }
}

fn tier_for(is_active: bool, stats: Option<&KeyStats>, target_cpm: f64) -> Tier {
    if !is_active {
        return Tier::Locked;
    }
    let Some(stats) = stats else {
        return Tier::Early;
    };
    let conf = stats.confidence(target_cpm);
    if conf >= 1.0 {
        Tier::Learned
    } else if conf >= 0.5 {
        Tier::Progressing
    } else {
        Tier::Early
    }
}

fn format_wpm(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{}", v.round() as i64),
        None => NO_SAMPLE.to_string(),
    }
}

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

    // Header — Key left-aligned, numerics right-aligned, Status left-aligned.
    let header = format!(
        "  {:<kw$}  {:>ww$}  {:>bw$}  {:>aw$}  {:>ew$}  {:<sw$}",
        "Key",
        "WPM",
        "Best",
        "Attempts",
        "Errors",
        "Status",
        kw = W_KEY,
        ww = W_WPM,
        bw = W_BEST,
        aw = W_ATTEMPTS,
        ew = W_ERRORS,
        sw = W_STATUS,
    );
    lines.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));

    let rule = format!(
        "  {:<kw$}  {:>ww$}  {:>bw$}  {:>aw$}  {:>ew$}  {:<sw$}",
        "-".repeat(W_KEY),
        "-".repeat(W_WPM),
        "-".repeat(W_BEST),
        "-".repeat(W_ATTEMPTS),
        "-".repeat(W_ERRORS),
        "-".repeat(W_STATUS),
        kw = W_KEY,
        ww = W_WPM,
        bw = W_BEST,
        aw = W_ATTEMPTS,
        ew = W_ERRORS,
        sw = W_STATUS,
    );
    lines.push(Line::from(Span::styled(
        rule,
        Style::default().fg(Color::DarkGray),
    )));

    for &key in UNLOCK_ORDER {
        let is_active = active_set.contains(&key);
        let is_focused = app.scheduler.focused_key == Some(key);

        let stats = app.per_key_stats.get(&key);
        let tier = tier_for(is_active, stats, app.target_cpm);

        let (wpm_str, best_str, attempts_str, errors_str) = match stats {
            Some(s) => (
                format_wpm(s.wpm()),
                format_wpm(s.best_wpm()),
                format!("{}", s.attempts),
                format!("{}", s.errors),
            ),
            None => (
                NO_SAMPLE.to_string(),
                NO_SAMPLE.to_string(),
                NO_SAMPLE.to_string(),
                NO_SAMPLE.to_string(),
            ),
        };

        let status_label = tier.label();
        let focus_marker = if is_focused { " < Focus" } else { "" };

        let row = format!(
            "  {:<kw$}  {:>ww$}  {:>bw$}  {:>aw$}  {:>ew$}  {:<sw$}{}",
            key,
            wpm_str,
            best_str,
            attempts_str,
            errors_str,
            status_label,
            focus_marker,
            kw = W_KEY,
            ww = W_WPM,
            bw = W_BEST,
            aw = W_ATTEMPTS,
            ew = W_ERRORS,
            sw = W_STATUS,
        );

        let mut style = Style::default().fg(tier.color());
        if is_focused {
            style = style.add_modifier(Modifier::BOLD);
        }

        lines.push(Line::from(Span::styled(row, style)));
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
