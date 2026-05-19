use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

/// All 26 ASCII lowercase letters in `a-z` order.
const ALPHABET: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// Map a key's best confidence (and unlock state) to a gradient color.
///
/// - `>= 1.00`            -> Green
/// - `>= 0.75`            -> LightGreen
/// - `>= 0.50`            -> Yellow
/// - `>= 0.25`            -> LightRed
/// - `< 0.25`, active     -> Red
/// - inactive (locked)    -> DarkGray
fn color_for(best_conf: f64, is_active: bool) -> Color {
    if !is_active {
        return Color::DarkGray;
    }
    if best_conf >= 1.0 {
        Color::Green
    } else if best_conf >= 0.75 {
        Color::LightGreen
    } else if best_conf >= 0.5 {
        Color::Yellow
    } else if best_conf >= 0.25 {
        Color::LightRed
    } else {
        Color::Red
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();
    let focused = app.scheduler.focused_key;

    // Build "a b c d ..." — single space between letters.
    let mut spans: Vec<Span> = Vec::with_capacity(ALPHABET.len() * 2);

    for (idx, &key) in ALPHABET.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }

        let is_active = active_set.contains(&key);
        let best_conf = app
            .per_key_stats
            .get(&key)
            .map(|s| s.best_confidence(app.target_cpm))
            .unwrap_or(0.0);

        let mut style = Style::default().fg(color_for(best_conf, is_active));
        if focused == Some(key) {
            style = style
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED);
        }

        spans.push(Span::styled(key.to_string(), style));
    }

    // Row width is `26 letters + 25 spaces = 51` chars. If it fits, center it;
    // otherwise just left-align (no wrap, no truncation).
    let row_width: u16 = (ALPHABET.len() * 2 - 1) as u16;
    let alignment = if area.width >= row_width {
        Alignment::Center
    } else {
        Alignment::Left
    };

    let para = Paragraph::new(Line::from(spans)).alignment(alignment);
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_keys_render_dark_gray() {
        assert_eq!(color_for(0.0, false), Color::DarkGray);
        assert_eq!(color_for(1.5, false), Color::DarkGray);
    }

    #[test]
    fn unlocked_unpracticed_renders_red() {
        // No stats yet (conf = 0.0) but key is active.
        assert_eq!(color_for(0.0, true), Color::Red);
        assert_eq!(color_for(0.24, true), Color::Red);
    }

    #[test]
    fn gradient_thresholds() {
        assert_eq!(color_for(0.25, true), Color::LightRed);
        assert_eq!(color_for(0.49, true), Color::LightRed);
        assert_eq!(color_for(0.50, true), Color::Yellow);
        assert_eq!(color_for(0.74, true), Color::Yellow);
        assert_eq!(color_for(0.75, true), Color::LightGreen);
        assert_eq!(color_for(0.99, true), Color::LightGreen);
        assert_eq!(color_for(1.0, true), Color::Green);
        assert_eq!(color_for(2.0, true), Color::Green);
    }
}
