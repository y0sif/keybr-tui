//! The top dashboard strip on the typing screen.
//!
//! This is keybr.com's "progress and session summary" card translated to a
//! TUI: four single-row sections stacked vertically, plus one row for the
//! ephemeral "letter unlocked!" callout.
//!
//! Layout (each row, label-aligned):
//!
//! ```text
//! Metrics:    94 wpm ↑+24  ·  99% acc ↑+3  ·  9,328 ↑+612
//! All keys:   █E█N█I█A█R▓L▓T▓O▒S▒D▒Y▒C·G·H·P·M·K·B·W·F·Z·V·X·Q·J
//! Current:    ░q   last 71 wpm 89%  ·  top 76 wpm 95%
//! Daily goal: ▓▓▓▓▓░░░░░  53% / 30 min
//! + 'b' unlocked!                       (only shown right after an unlock)
//! ```
//!
//! All deltas read from `App::lesson_history`, which is an in-memory rolling
//! window (not persisted); when there's not enough history yet the delta
//! column is suppressed.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{lesson_score, App};
use crate::components::key_bar;

/// Width reserved for row labels (e.g. "Metrics:    "). Keeps section
/// values left-aligned in a consistent column across all four rows.
const LABEL_WIDTH: usize = 12;

/// Width of the daily-goal bar in cells.
const GOAL_BAR_CELLS: u32 = 10;

/// Total width of the dashboard's content band: 12 cols for the label
/// plus 78 cols for 26 three-cell heatmap tiles. All four rows align
/// to this band so the labels sit in the same column on every line.
const DASHBOARD_WIDTH: u16 = LABEL_WIDTH as u16 + 26 * key_bar::TILE_WIDTH;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // 5 single-line rows: 4 dashboard sections + 1 callout row that's
    // either blank or shows the "+letter unlocked!" message.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Metrics
            Constraint::Length(1), // All keys
            Constraint::Length(1), // Current key
            Constraint::Length(1), // Daily goal
            Constraint::Length(1), // Callout (unlock) / blank
        ])
        .split(area);

    render_metrics_row(app, frame, rows[0]);
    render_keys_row(app, frame, rows[1]);
    render_current_key_row(app, frame, rows[2]);
    render_daily_goal_row(app, frame, rows[3]);
    render_callout_row(app, frame, rows[4]);
}

/// Build a styled label like "Metrics:    " left-padded to `LABEL_WIDTH`.
fn label_span(text: &str) -> Span<'static> {
    let label = format!("{:<width$}", text, width = LABEL_WIDTH);
    Span::styled(label, Style::default().fg(Color::DarkGray))
}

/// The dashboard's content sits in a fixed-width band centered in the
/// frame. The band is sized to fit the widest row (the heatmap), so
/// every other row's label lines up underneath the heatmap's label and
/// nothing gets clipped on standard-width terminals.
fn centered_inner(area: Rect) -> Rect {
    let inner_w = DASHBOARD_WIDTH.min(area.width);
    let left_margin = area.width.saturating_sub(inner_w) / 2;
    Rect {
        x: area.x + left_margin,
        y: area.y,
        width: inner_w,
        height: area.height,
    }
}

/// Render one row by combining the label and its content spans.
fn render_row(frame: &mut Frame, area: Rect, mut spans: Vec<Span<'static>>) {
    let inner = centered_inner(area);
    let mut all = Vec::with_capacity(spans.len() + 1);
    all.append(&mut spans);
    let para = Paragraph::new(Line::from(all)).alignment(Alignment::Left);
    frame.render_widget(para, inner);
}

// ─── Row 1: Metrics ─────────────────────────────────────────────────────

fn render_metrics_row(app: &App, frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let strong = Style::default().fg(Color::White);
    let sep = Span::styled("  ·  ", dim);

    let mut spans: Vec<Span<'static>> = vec![label_span("Metrics:")];

    // The metrics row reflects the most recently *completed* lesson — never
    // in-progress values. Falling back to live `lesson_wpm()` / `lesson_accuracy()`
    // here made the row flicker every keystroke on the first lesson, which
    // read as buggy. Until at least one lesson is done, show placeholders.
    let Some(last) = &app.last_lesson else {
        spans.push(Span::styled("—  wpm", dim));
        spans.push(sep.clone());
        spans.push(Span::styled("—  acc", dim));
        spans.push(sep);
        spans.push(Span::styled("—", dim));
        render_row(frame, area, spans);
        return;
    };

    let wpm = last.wpm;
    let accuracy = last.accuracy;
    let score = lesson_score(wpm, accuracy);

    spans.push(Span::styled(format!("{:>3.0} wpm", wpm), strong));
    if let Some(d) = delta_span(wpm, app.prev_mean_wpm()) {
        spans.push(Span::raw(" "));
        spans.push(d);
    }
    spans.push(sep.clone());
    spans.push(Span::styled(format!("{:>3.0}% acc", accuracy), strong));
    if let Some(d) = delta_span(accuracy, app.prev_mean_accuracy()) {
        spans.push(Span::raw(" "));
        spans.push(d);
    }
    spans.push(sep);
    spans.push(Span::styled(format_score(score), strong));
    if let Some(d) = delta_span(score, app.prev_mean_score()) {
        spans.push(Span::raw(" "));
        spans.push(d);
    }

    render_row(frame, area, spans);
}

/// "12,345" — comma-grouped score for readability.
fn format_score(score: f64) -> String {
    let n = score.max(0.0) as u64;
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Build the "↑+24" or "↓-3" delta span, or `None` when there's no
/// baseline yet (first lesson). Green for an improvement vs the rolling
/// mean, red for a regression, dim when the delta rounds to zero.
fn delta_span(current: f64, baseline: Option<f64>) -> Option<Span<'static>> {
    let baseline = baseline?;
    let diff = current - baseline;
    // Treat near-zero as "no change" so we don't render misleading arrows
    // for floating-point dust.
    let rounded = diff.round();
    let (arrow, color) = if rounded > 0.0 {
        ("↑+", Color::Green)
    } else if rounded < 0.0 {
        ("↓", Color::Red)
    } else {
        ("·", Color::DarkGray)
    };
    let abs = rounded.abs();
    let text = if rounded == 0.0 {
        format!("({} 0)", arrow)
    } else if abs >= 1000.0 {
        format!("({}{:.1}k)", arrow, abs / 1000.0)
    } else {
        format!("({}{:.0})", arrow, abs)
    };
    Some(Span::styled(text, Style::default().fg(color)))
}

// ─── Row 2: All keys ────────────────────────────────────────────────────

fn render_keys_row(app: &App, frame: &mut Frame, area: Rect) {
    // The label sits in the centered "label column"; the heatmap tile
    // row gets the rest of the centered band. Split the inner area so
    // the label and the tiles share the same horizontal rhythm as the
    // other rows.
    let inner = centered_inner(area);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LABEL_WIDTH as u16),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Line::from(label_span("All keys:"))).alignment(Alignment::Left);
    frame.render_widget(label, cols[0]);

    // Delegate to key_bar — it already knows how to lay out 26 tiles and
    // handle the focused key, so we just hand it the right sub-rect.
    key_bar::render(app, frame, cols[1]);
}

// ─── Row 3: Current key ─────────────────────────────────────────────────

fn render_current_key_row(app: &App, frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let strong = Style::default().fg(Color::White);
    let sep = Span::styled("  ·  ", dim);

    let mut spans: Vec<Span<'static>> = vec![label_span("Current:")];

    let Some(focused) = app.scheduler.focused_key else {
        // No focused key (every active key is "learned"). Say so
        // explicitly rather than leaving the row blank — that's a
        // milestone worth highlighting.
        spans.push(Span::styled("all keys at target speed", dim));
        render_row(frame, area, spans);
        return;
    };

    let stats = app.per_key_stats.get(&focused);
    let best_conf = stats
        .map(|s| s.best_confidence(app.target_cpm))
        .unwrap_or(0.0);
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();
    let is_active = active_set.contains(&focused);

    spans.extend(key_bar::key_tile_spans(focused, best_conf, is_active, true));
    spans.push(Span::raw("   "));

    // "Last" pair: current smoothed wpm + current confidence as % of target.
    spans.push(Span::styled("last ", dim));
    match stats.and_then(|s| s.wpm()) {
        Some(wpm) => {
            spans.push(Span::styled(format!("{:.0} wpm", wpm), strong));
            if let Some(cur_conf) = stats.map(|s| s.confidence(app.target_cpm)) {
                spans.push(Span::styled(
                    format!(" {:.0}%", (cur_conf * 100.0).min(999.0)),
                    dim,
                ));
            }
        }
        None => spans.push(Span::styled("—", dim)),
    }

    spans.push(sep);

    // "Top" pair: historical best wpm + best confidence as % of target.
    spans.push(Span::styled("top ", dim));
    match stats.and_then(|s| s.best_wpm()) {
        Some(best_wpm) => {
            spans.push(Span::styled(format!("{:.0} wpm", best_wpm), strong));
            spans.push(Span::styled(
                format!(" {:.0}%", (best_conf * 100.0).min(999.0)),
                dim,
            ));
        }
        None => spans.push(Span::styled("—", dim)),
    }

    render_row(frame, area, spans);
}

// ─── Row 4: Daily goal ──────────────────────────────────────────────────

fn render_daily_goal_row(app: &App, frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);

    if app.daily_goal_minutes == 0 {
        // User turned the goal off — show that state explicitly so the
        // row doesn't look like a layout glitch.
        let spans = vec![label_span("Daily goal:"), Span::styled("off", dim)];
        render_row(frame, area, spans);
        return;
    }

    let goal_min = app.daily_goal_minutes;
    let practiced_secs = app.today_seconds_practiced;
    let goal_secs = goal_min.saturating_mul(60);
    let pct = if goal_secs == 0 {
        0.0
    } else {
        (practiced_secs as f64 / goal_secs as f64).clamp(0.0, 1.0)
    };
    let filled_cells = (pct * GOAL_BAR_CELLS as f64).round() as u32;
    let filled_cells = filled_cells.min(GOAL_BAR_CELLS);

    // Color the filled portion green once we've met the goal, otherwise
    // a soft white so the bar reads as "in progress".
    let filled_color = if filled_cells >= GOAL_BAR_CELLS {
        Color::Green
    } else {
        Color::White
    };

    let filled_str: String = "▓".repeat(filled_cells as usize);
    let empty_str: String = "░".repeat((GOAL_BAR_CELLS - filled_cells) as usize);

    let pct_text = format!("  {:>3.0}% / {} min", pct * 100.0, goal_min);

    let spans = vec![
        label_span("Daily goal:"),
        Span::styled(filled_str, Style::default().fg(filled_color)),
        Span::styled(empty_str, dim),
        Span::styled(pct_text, dim),
    ];
    render_row(frame, area, spans);
}

// ─── Row 5: unlock callout (ephemeral) ─────────────────────────────────

fn render_callout_row(app: &App, frame: &mut Frame, area: Rect) {
    let unlocked = app.last_lesson.as_ref().and_then(|r| r.newly_unlocked);
    let Some(ch) = unlocked else {
        return; // blank row when there's nothing to celebrate
    };
    let spans = vec![
        label_span(""), // empty label keeps the column rhythm
        Span::styled(
            format!("+ '{}' unlocked!", ch),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    render_row(frame, area, spans);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_score_inserts_thousands_separators() {
        assert_eq!(format_score(0.0), "0");
        assert_eq!(format_score(42.0), "42");
        assert_eq!(format_score(1_234.0), "1,234");
        assert_eq!(format_score(12_345.0), "12,345");
        assert_eq!(format_score(1_234_567.0), "1,234,567");
    }

    #[test]
    fn delta_span_is_none_without_baseline() {
        assert!(delta_span(50.0, None).is_none());
    }

    #[test]
    fn delta_span_positive_uses_green() {
        let s = delta_span(75.0, Some(50.0)).expect("delta");
        assert!(s.content.contains("↑+"));
        assert_eq!(s.style.fg, Some(Color::Green));
    }

    #[test]
    fn delta_span_negative_uses_red() {
        let s = delta_span(40.0, Some(50.0)).expect("delta");
        assert!(s.content.contains("↓"));
        assert_eq!(s.style.fg, Some(Color::Red));
    }

    #[test]
    fn delta_span_zero_is_dim() {
        let s = delta_span(50.4, Some(50.0)).expect("delta");
        // Rounded difference is 0 — render the "no change" form.
        assert!(s.content.contains("0"));
        assert_eq!(s.style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn delta_span_large_value_uses_k_suffix() {
        let s = delta_span(7_500.0, Some(1_000.0)).expect("delta");
        assert!(s.content.contains("k"));
    }
}
