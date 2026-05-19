use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, ErrorMode};

/// Build the 10-cell daily-goal bar as styled spans:
/// filled cells use the regular foreground; empty cells use `Color::DarkGray`.
fn goal_bar_spans(progressed: u32, goal: u32) -> Vec<Span<'static>> {
    const CELLS: u32 = 10;
    let ratio = (progressed as f64 / goal as f64).clamp(0.0, 1.0);
    let filled = (ratio * CELLS as f64).round() as u32;
    let filled = filled.min(CELLS);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(2);
    if filled > 0 {
        let s: String = "█".repeat(filled as usize);
        spans.push(Span::styled(s, Style::default()));
    }
    if filled < CELLS {
        let s: String = "░".repeat((CELLS - filled) as usize);
        spans.push(Span::styled(s, Style::default().fg(Color::DarkGray)));
    }
    spans
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    // Two-line vertical layout: stats on top, focus + controls + goal on bottom.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // ── Top line: lesson info, WPM, accuracy, progress ──
    let lesson_label = format!("Lesson {}", app.lesson_count);
    let wpm_label = if let Some(last) = &app.last_lesson {
        format!("{:.0} wpm", last.wpm)
    } else {
        format!("{:.0} wpm", app.lesson_wpm())
    };
    let acc_label = format!("{:.0}% acc", app.lesson_accuracy());

    let total_chars = app.generated_text.chars().count();
    let progress_label = format!("{}/{} chars", app.cursor_pos, total_chars);

    let top = Paragraph::new(Line::from(vec![Span::styled(
        format!(
            " {} | {} | {} | {}",
            lesson_label, wpm_label, acc_label, progress_label
        ),
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(top, rows[0]);

    // ── Bottom line: focus + controls (left), daily-goal bar (right) ──
    let show_goal = app.daily_goal_minutes > 0;
    let bottom_cols = if show_goal {
        // Reserve a fixed-width column on the right for the goal bar.
        // "Today: NNN/NNN min ░░░░░░░░░░" → up to ~30 chars; give it 32.
        Layout::horizontal([Constraint::Min(0), Constraint::Length(32)]).split(rows[1])
    } else {
        Layout::horizontal([Constraint::Min(0), Constraint::Length(0)]).split(rows[1])
    };

    let focused_label = match app.scheduler.focused_key {
        Some(k) => {
            let conf = app
                .per_key_stats
                .get(&k)
                .map(|s| (s.confidence(app.target_cpm) * 100.0).round() as u32)
                .unwrap_or(0);
            format!("Focus: '{}' ({}%)", k, conf)
        }
        None => "Focus: -".to_string(),
    };
    let mode_label = match app.error_mode {
        ErrorMode::ForgiveMistakes => "forgive",
        ErrorMode::StopOnError => "stop",
    };

    let left = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {}   [Tab] {} · [Esc] menu", focused_label, mode_label),
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(left, bottom_cols[0]);

    if show_goal {
        let practiced = app.today_minutes_practiced;
        let goal = app.daily_goal_minutes;
        let display_practiced = practiced.min(goal);
        let prefix = format!("Today: {}/{} min ", display_practiced, goal);

        let mut spans: Vec<Span<'static>> =
            vec![Span::styled(prefix, Style::default().fg(Color::DarkGray))];
        spans.extend(goal_bar_spans(practiced, goal));
        // Trailing space so the bar isn't flush against the right edge.
        spans.push(Span::raw(" "));

        let right = Paragraph::new(Line::from(spans)).alignment(Alignment::Right);
        frame.render_widget(right, bottom_cols[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_bar_empty_when_no_progress() {
        let spans = goal_bar_spans(0, 30);
        // No filled cells; one empty-cell span of length 10.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.chars().count(), 10);
    }

    #[test]
    fn goal_bar_full_when_complete() {
        let spans = goal_bar_spans(30, 30);
        // All filled; single span of 10 filled cells.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.chars().count(), 10);
    }

    #[test]
    fn goal_bar_clamps_when_over_goal() {
        // 60/30 is 200% — should clamp to a full 10-cell bar, no overflow.
        let spans = goal_bar_spans(60, 30);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.chars().count(), 10);
    }

    #[test]
    fn goal_bar_partial_fill() {
        // 12/30 ≈ 40% → 4 filled + 6 empty.
        let spans = goal_bar_spans(12, 30);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.chars().count(), 4);
        assert_eq!(spans[1].content.chars().count(), 6);
    }
}
