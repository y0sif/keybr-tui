use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, ErrorMode};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let columns = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(area);

    // Left: lesson info, WPM, accuracy
    let lesson_label = format!("Lesson {}", app.lesson_count);
    let wpm_label = if let Some(last) = &app.last_lesson {
        format!("{:.0} wpm", last.wpm)
    } else {
        format!("{:.0} wpm", app.lesson_wpm())
    };
    let acc_label = format!("{:.0}% acc", app.lesson_accuracy());

    let total_chars = app.generated_text.chars().count();
    let progress_label = format!("{}/{} chars", app.cursor_pos, total_chars);

    let left = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} | {} | {} | {}", lesson_label, wpm_label, acc_label, progress_label),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    frame.render_widget(left, columns[0]);

    // Center: focused key
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
    let center = Paragraph::new(Line::from(vec![Span::styled(
        focused_label,
        Style::default().fg(Color::DarkGray),
    )]))
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(center, columns[1]);

    // Right: controls
    let mode_label = match app.error_mode {
        ErrorMode::ForgiveMistakes => "forgive",
        ErrorMode::StopOnError => "stop",
    };
    let right = Paragraph::new(Line::from(vec![Span::styled(
        format!("[Tab] {} | [Esc] menu ", mode_label),
        Style::default().fg(Color::DarkGray),
    )]))
    .alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(right, columns[2]);
}
