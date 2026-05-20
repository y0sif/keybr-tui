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
/// - `>= 1.00`            -> Green        (learned)
/// - `>= 0.75`            -> LightGreen   (strong)
/// - `>= 0.50`            -> Yellow       (progressing)
/// - `>= 0.25`            -> LightRed     (early)
/// - `< 0.25`, active     -> Red          (struggling / unpracticed)
/// - inactive (locked)    -> DarkGray
pub fn color_for(best_conf: f64, is_active: bool) -> Color {
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

/// Build the 2-cell tile for a single key — a colored cell containing
/// the uppercased letter with a leading space of padding. Uses a real
/// terminal background color so the heatmap actually reads like
/// keybr.com's tiles (the prior shade-character approach was hard to
/// scan because the block sat *next to* the letter rather than under it).
/// Background colors are intentionally scoped to the heatmap only — the
/// typing area still defers to the terminal background per the brand rule.
pub fn key_tile_spans(
    key: char,
    best_conf: f64,
    is_active: bool,
    is_focused: bool,
) -> Vec<Span<'static>> {
    let bg = color_for(best_conf, is_active);
    let letter = key.to_ascii_uppercase().to_string();
    // White foreground reads well against every ANSI color used by
    // `color_for` (green, light-green, yellow, light-red, red, dark-gray).
    let mut style = Style::default().bg(bg).fg(Color::White);
    if is_focused {
        style = style
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::UNDERLINED);
    }

    vec![
        Span::styled(" ", style),
        Span::styled(letter, style),
    ]
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();
    let focused = app.scheduler.focused_key;

    // Each key takes 2 cells (block + letter). The all-keys row is
    // visually denser than the old space-separated layout, so we don't
    // need extra padding between tiles.
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(ALPHABET.len() * 2);

    for &key in ALPHABET.iter() {
        let is_active = active_set.contains(&key);
        let best_conf = app
            .per_key_stats
            .get(&key)
            .map(|s| s.best_confidence(app.target_cpm))
            .unwrap_or(0.0);
        let is_focused = focused == Some(key);

        spans.extend(key_tile_spans(key, best_conf, is_active, is_focused));
    }

    // 26 letters × 2 cells = 52 chars; center when it fits, otherwise
    // left-align so the leading tiles stay visible if the terminal is
    // narrow (we'd rather show ENIAR than nothing).
    let row_width: u16 = (ALPHABET.len() * 2) as u16;
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

    #[test]
    fn key_tile_spans_emits_padded_letter_with_bg() {
        let spans = key_tile_spans('e', 1.0, true, false);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, " ");
        assert_eq!(spans[1].content, "E");
        // Background carries the confidence color so the cell reads as a tile.
        assert_eq!(spans[1].style.bg, Some(Color::Green));
        assert_eq!(spans[1].style.fg, Some(Color::White));
    }

    #[test]
    fn locked_tile_uses_dark_gray_bg() {
        let spans = key_tile_spans('z', 0.0, false, false);
        assert_eq!(spans[0].style.bg, Some(Color::DarkGray));
        assert_eq!(spans[1].style.bg, Some(Color::DarkGray));
    }

    #[test]
    fn focused_tile_is_bold_and_underlined() {
        let spans = key_tile_spans('e', 1.0, true, true);
        let modifiers = spans[1].style.add_modifier;
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::UNDERLINED));
    }
}
