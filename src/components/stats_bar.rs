//! Shared helpers for stats-related rendering.
//!
//! The full bottom stats bar has been replaced by an inline progress panel
//! rendered directly from `ui.rs`; this module now only exposes the
//! daily-goal bar builder, which that panel imports.

use ratatui::{
    style::{Color, Style},
    text::Span,
};

/// Build the 10-cell daily-goal bar as styled spans:
/// filled cells use the regular foreground; empty cells use `Color::DarkGray`.
pub fn goal_bar_spans(progressed: u32, goal: u32) -> Vec<Span<'static>> {
    const CELLS: u32 = 10;
    let ratio = if goal == 0 {
        0.0
    } else {
        (progressed as f64 / goal as f64).clamp(0.0, 1.0)
    };
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
