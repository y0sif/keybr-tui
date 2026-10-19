use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::engine::Language;

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

/// Width of one heatmap tile in terminal cells. Three cells (space +
/// letter + space) puts the letter in the visual center of the colored
/// block. Exposed so the dashboard can size its content band correctly.
pub const TILE_WIDTH: u16 = 3;

/// Build the 3-cell tile for a single key — `" X "` with a real
/// terminal background color so the letter sits centered inside the
/// colored block. Background colors are intentionally scoped to the
/// heatmap only — the typing area still defers to the terminal
/// background per the brand rule.
///
/// Always three spans (pad, letter, pad) and `TILE_WIDTH` cells: the one
/// two-cell label, Armenian `ԵՎ`, gets a single pad cell after it, so the
/// row math in `rows_for` holds for every language.
pub fn key_tile_spans(
    language: Language,
    key: char,
    best_conf: f64,
    is_active: bool,
    is_focused: bool,
    is_pinned: bool,
) -> Vec<Span<'static>> {
    let bg = color_for(best_conf, is_active);
    let letter = language.upper(key);
    // White foreground reads well against every ANSI color used by
    // `color_for` (green, light-green, yellow, light-red, red, dark-gray).
    let mut style = Style::default().bg(bg).fg(Color::White);
    if is_focused {
        style = style
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::UNDERLINED);
        // A manual pin inverts the tile on top of the focus styling so
        // it reads differently from the scheduler's auto pick.
        if is_pinned {
            style = style.add_modifier(Modifier::REVERSED);
        }
    }

    let label = Span::styled(letter, style);
    let pad = (TILE_WIDTH as usize).saturating_sub(label.width());
    vec![
        Span::styled(" ".repeat(pad / 2), style),
        label,
        Span::styled(" ".repeat(pad - pad / 2), style),
    ]
}

/// Terminal rows the heatmap needs to show `letter_count` tiles in
/// `available_width` cells. One when the single row fits, so a wide
/// terminal renders exactly as it always has; otherwise the fewest rows
/// that fit. `render` spreads the tiles evenly over those rows
/// (`letter_count.div_ceil(rows)` per row), which never exceeds the width
/// and fills exactly this many rows. A width too narrow for even one tile
/// stays at one clipped row: more rows would show nothing more.
pub fn rows_for(letter_count: usize, available_width: u16) -> u16 {
    let per_row = (available_width / TILE_WIDTH) as usize;
    if per_row == 0 || letter_count <= per_row {
        return 1;
    }
    letter_count.div_ceil(per_row) as u16
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let active_set: std::collections::HashSet<char> =
        app.scheduler.active_keys.iter().copied().collect();
    // Every letter the focus rule involves lights up, so a combination
    // drill highlights both halves of its reach and a single-key focus
    // stays the one-tile case it always was.
    let focus_letters: Vec<char> = app
        .effective_focus()
        .map(|rule| rule.letters())
        .unwrap_or_default();
    // A stale pin falls back to auto inside `focus_is_pinned` and
    // renders as such.
    let focus_is_pinned = app.focus_is_pinned();

    // Each key takes 3 cells (space + letter + space). Adjacent tiles
    // share no gap so the heatmap reads as one continuous colored band,
    // matching keybr.com's tile grid.
    // One tile per letter of the practice language, in alphabetical order.
    let letters = app.language.letters();
    let tiles: Vec<Vec<Span<'static>>> = letters
        .iter()
        .map(|&key| {
            let is_active = active_set.contains(&key);
            let best_conf = app
                .per_key_stats
                .get(&key)
                .map(|s| s.best_confidence(app.target_cpm))
                .unwrap_or(0.0);
            let is_focused = focus_letters.contains(&key);

            key_tile_spans(
                app.language,
                key,
                best_conf,
                is_active,
                is_focused,
                is_focused && focus_is_pinned,
            )
        })
        .collect();

    // 3 cells per letter: 78 for English, 87 for Turkish, 90 for German.
    // A row too wide for the area is split into balanced rows (the
    // dashboard reserves `rows_for` of them) rather than cut off at the
    // right edge. ratatui's `Wrap` can't do it: it breaks on whitespace,
    // and every tile has a space inside it.
    let rows = rows_for(letters.len(), area.width);
    let per_row = letters.len().div_ceil(rows as usize).max(1);
    let lines: Vec<Line<'static>> = tiles
        .chunks(per_row)
        .map(|row| Line::from(row.concat()))
        .collect();

    // Center the single row when it fits. Wrapped rows are left-aligned,
    // so each tile column starts at the same x on every row.
    let row_width: u16 = letters.len() as u16 * TILE_WIDTH;
    let alignment = if area.width >= row_width {
        Alignment::Center
    } else {
        Alignment::Left
    };

    let para = Paragraph::new(lines).alignment(alignment);
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
    fn key_tile_spans_emits_three_cells_with_letter_centered() {
        let spans = key_tile_spans(Language::English, 'e', 1.0, true, false, false);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, " ");
        assert_eq!(spans[1].content, "E");
        assert_eq!(spans[2].content, " ");
        // Background carries the confidence color so the cell reads as a tile.
        assert_eq!(spans[1].style.bg, Some(Color::Green));
        assert_eq!(spans[1].style.fg, Some(Color::White));
    }

    #[test]
    fn two_letter_tile_still_takes_three_cells() {
        let spans = key_tile_spans(Language::Armenian, 'և', 1.0, true, false, false);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "");
        assert_eq!(spans[1].content, "ԵՎ");
        assert_eq!(spans[2].content, " ");
        // Every tile of every language is exactly one tile wide.
        for &language in Language::ALL {
            for &key in language.letters() {
                let spans = key_tile_spans(language, key, 0.0, true, true, true);
                let width: usize = spans.iter().map(Span::width).sum();
                assert_eq!(width, TILE_WIDTH as usize, "{language:?} '{key}'");
            }
        }
    }

    #[test]
    fn rows_for_keeps_one_row_while_it_fits() {
        // Each language's row exactly fits, then needs a second row one
        // cell narrower.
        for (letters, row_width) in [(26, 78), (29, 87), (30, 90)] {
            assert_eq!(rows_for(letters, row_width), 1, "{letters} letters");
            assert_eq!(rows_for(letters, 200), 1, "{letters} letters");
            assert_eq!(rows_for(letters, row_width - 1), 2, "{letters} letters");
        }
    }

    #[test]
    fn rows_for_degenerate_widths() {
        for letters in [26, 29, 30] {
            // Not even one tile fits: one clipped row, not a taller one.
            for width in 0..TILE_WIDTH {
                assert_eq!(rows_for(letters, width), 1, "{letters} at {width}");
            }
            for width in TILE_WIDTH..=12 {
                let per_row = (width / TILE_WIDTH) as usize;
                assert_eq!(
                    rows_for(letters, width) as usize,
                    letters.div_ceil(per_row),
                    "{letters} at {width}"
                );
            }
        }
        assert_eq!(rows_for(0, 0), 1);
        assert_eq!(rows_for(0, 80), 1);
    }

    /// The contract `render` and the dashboard's height rely on: the
    /// balanced split fits the width, fills exactly `rows_for` rows, and
    /// one row fewer would not fit.
    #[test]
    fn rows_for_balanced_split_fits_and_is_fewest() {
        for letters in 1..=40usize {
            for width in TILE_WIDTH..=130 {
                let rows = rows_for(letters, width) as usize;
                let per_row = letters.div_ceil(rows);
                assert!(per_row as u16 * TILE_WIDTH <= width, "{letters} at {width}");
                assert_eq!(letters.div_ceil(per_row), rows, "{letters} at {width}");
                if rows > 1 {
                    let fewer = letters.div_ceil(rows - 1);
                    assert!(fewer as u16 * TILE_WIDTH > width, "{letters} at {width}");
                }
            }
        }
    }

    #[test]
    fn locked_tile_uses_dark_gray_bg() {
        let spans = key_tile_spans(Language::English, 'z', 0.0, false, false, false);
        assert_eq!(spans[0].style.bg, Some(Color::DarkGray));
        assert_eq!(spans[1].style.bg, Some(Color::DarkGray));
        assert_eq!(spans[2].style.bg, Some(Color::DarkGray));
    }

    #[test]
    fn focused_tile_is_bold_and_underlined() {
        let spans = key_tile_spans(Language::English, 'e', 1.0, true, true, false);
        let modifiers = spans[1].style.add_modifier;
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::UNDERLINED));
        assert!(
            !modifiers.contains(Modifier::REVERSED),
            "auto focus must not render as a pin"
        );
    }

    #[test]
    fn pinned_tile_adds_reversed_over_focus_styling() {
        let spans = key_tile_spans(Language::English, 'e', 1.0, true, true, true);
        let modifiers = spans[1].style.add_modifier;
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::UNDERLINED));
        assert!(modifiers.contains(Modifier::REVERSED));
    }

    #[test]
    fn turkish_tiles_use_turkish_uppercase() {
        let text = |spans: Vec<Span<'static>>| spans[1].content.to_string();
        assert_eq!(
            text(key_tile_spans(
                Language::Turkish,
                'i',
                1.0,
                true,
                false,
                false
            )),
            "İ"
        );
        assert_eq!(
            text(key_tile_spans(
                Language::Turkish,
                'ı',
                1.0,
                true,
                false,
                false
            )),
            "I"
        );
        assert_eq!(
            text(key_tile_spans(
                Language::Turkish,
                'ş',
                1.0,
                true,
                false,
                false
            )),
            "Ş"
        );
        assert_eq!(
            text(key_tile_spans(
                Language::English,
                'i',
                1.0,
                true,
                false,
                false
            )),
            "I"
        );
    }
}
