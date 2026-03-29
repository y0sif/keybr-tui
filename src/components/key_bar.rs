use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::engine::scheduler::UNLOCK_ORDER;

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Style::default()));

    let mut past_active = false;

    for &key in UNLOCK_ORDER {
        let is_active = active_set.contains(&key);

        if !is_active && !past_active {
            past_active = true;
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        } else {
            spans.push(Span::styled(" ", Style::default()));
        }

        if is_active {
            let is_focused = app.scheduler.focused_key == Some(key);
            let conf = app
                .per_key_stats
                .get(&key)
                .map(|s| s.confidence(app.target_cpm))
                .unwrap_or(0.0);

            let conf_pct = (conf * 100.0).round() as u32;

            let color = if conf >= 1.0 {
                Color::Green
            } else if conf >= 0.5 {
                Color::Yellow
            } else {
                Color::White
            };

            let mut style = Style::default().fg(color);
            if is_focused {
                style = style
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED);
            }

            let label = format!("{}({}%)", key, conf_pct);
            spans.push(Span::styled(label, style));
        } else {
            let style = Style::default().fg(Color::DarkGray);
            spans.push(Span::styled(key.to_string(), style));
        }
    }

    let para = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    frame.render_widget(para, area);
}
