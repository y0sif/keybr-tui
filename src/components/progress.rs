use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
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

/// Rows the header and its rule take at the top of the table.
const HEADER_ROWS: u16 = 2;

/// Progress tier derived from confidence vs. target CPM.
/// `Locked` keys haven't been unlocked by the scheduler yet.
/// `pub(crate)` so the taria tree (`crate::tree`, unix only) reports the
/// same tiers the human sees, from the same thresholds.
pub(crate) enum Tier {
    Locked,
    Early,
    Progressing,
    Learned,
}

impl Tier {
    pub(crate) fn label(&self) -> &'static str {
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

pub(crate) fn tier_for(is_active: bool, stats: Option<&KeyStats>, target_cpm: f64) -> Tier {
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

/// Title, table and footer. Shared by `render` and `scroll_limits`, so the
/// scroll clamp in `update` measures the same table the frame draws.
fn screen_layout(area: Rect) -> [Rect; 3] {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .areas(area)
}

/// How many letter rows a table `height` rows tall shows at once, or
/// `None` when all `letters` fit under the header. A list that does not
/// fit gives up one more row, to the "more" line below the letters.
fn scrolling_rows(height: u16, letters: usize) -> Option<usize> {
    let space = usize::from(height.saturating_sub(HEADER_ROWS));
    (letters > space).then(|| space.saturating_sub(1).max(1))
}

/// The letter table's scroll limits on this app's terminal, as
/// `(page, max)`: how far PageUp/PageDown move, and the offset that brings
/// the last letter into view. A list that fits has a `max` of 0.
///
/// `update` has no frame to measure, so this lays the screen out at
/// `app.terminal_height`; `render` measures the frame it is given.
pub(crate) fn scroll_limits(app: &App) -> (usize, usize) {
    let letters = app.language.unlock_order().len();
    let [_, table, _] = screen_layout(Rect::new(0, 0, 1, app.terminal_height));
    match scrolling_rows(table.height, letters) {
        Some(rows) => (rows, letters - rows),
        None => (letters, 0),
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let v_chunks = screen_layout(area);

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

    // A combination drill marks every letter it involves, so the "< Focus"
    // column reads the same way for a bigram as for a single key.
    let focus_letters: Vec<char> = app
        .effective_focus()
        .map(|rule| rule.letters())
        .unwrap_or_default();
    // A list taller than the table scrolls. The offset is clamped here as
    // well as in `update`, since a resize that makes room for more rows
    // lowers the limit between keys.
    let letters = app.language.unlock_order();
    let scrolling = scrolling_rows(inner.height, letters.len());
    let first = scrolling.map_or(0, |rows| app.progress_scroll.min(letters.len() - rows));
    let shown = &letters[first..first + scrolling.unwrap_or(letters.len())];
    for &key in shown {
        let is_active = active_set.contains(&key);
        let is_focused = focus_letters.contains(&key);

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

    // Say how many letters are out of view, and on which side.
    if scrolling.is_some() {
        let above = first;
        let below = letters.len() - first - shown.len();
        let mut more = Vec::new();
        if above > 0 {
            more.push(format!("↑ {above} more"));
        }
        if below > 0 {
            more.push(format!("↓ {below} more"));
        }
        lines.push(Line::from(Span::styled(
            format!("  {}", more.join("    ")),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let table = Paragraph::new(Text::from(lines)).alignment(Alignment::Left);
    frame.render_widget(table, inner);

    // Footer. The scroll keys only get a mention when there is something
    // to scroll.
    let scroll_hint = if scrolling.is_some() {
        "[Up/Down] Scroll    "
    } else {
        ""
    };
    let footer_text = format!(
        "  Lessons completed: {}    {scroll_hint}[Esc] Back to menu",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppScreen, ErrorMode};
    use crate::engine::Language;
    use crate::events::AppEvent;
    use crate::update::update;
    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        Terminal,
    };

    /// A fresh profile on the Progress screen of a terminal `height` rows
    /// tall.
    fn progress_app(language: Language, height: u16) -> App {
        let mut app = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, language);
        app.screen = AppScreen::Progress;
        app.terminal_height = height;
        app
    }

    fn draw(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| render(app, frame, frame.area()))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    fn press(app: &mut App, code: KeyCode) {
        update(app, AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// A fresh English profile as the screen drew it before the table
    /// could scroll, on the shortest terminal that holds all 26 letters
    /// (80 x 33). `~` marks a `NO_SAMPLE` cell; the last two rows are blank.
    const FITTING_ENGLISH: [&str; 31] = [
        "                                  Key Progress",
        "",
        "          Key    WPM   Best    Attempts    Errors  Status",
        "          ---  -----  -----  ----------  --------  ------------",
        "          e        ~      ~           ~         ~  Early        < Focus",
        "          n        ~      ~           ~         ~  Early",
        "          i        ~      ~           ~         ~  Early",
        "          a        ~      ~           ~         ~  Early",
        "          r        ~      ~           ~         ~  Early",
        "          l        ~      ~           ~         ~  Early",
        "          t        ~      ~           ~         ~  Locked",
        "          o        ~      ~           ~         ~  Locked",
        "          s        ~      ~           ~         ~  Locked",
        "          u        ~      ~           ~         ~  Locked",
        "          d        ~      ~           ~         ~  Locked",
        "          y        ~      ~           ~         ~  Locked",
        "          c        ~      ~           ~         ~  Locked",
        "          g        ~      ~           ~         ~  Locked",
        "          h        ~      ~           ~         ~  Locked",
        "          p        ~      ~           ~         ~  Locked",
        "          m        ~      ~           ~         ~  Locked",
        "          k        ~      ~           ~         ~  Locked",
        "          b        ~      ~           ~         ~  Locked",
        "          w        ~      ~           ~         ~  Locked",
        "          f        ~      ~           ~         ~  Locked",
        "          z        ~      ~           ~         ~  Locked",
        "          v        ~      ~           ~         ~  Locked",
        "          x        ~      ~           ~         ~  Locked",
        "          q        ~      ~           ~         ~  Locked",
        "          j        ~      ~           ~         ~  Locked",
        "                    Lessons completed: 0    [Esc] Back to menu",
    ];

    /// A list that fits draws exactly what it did before scrolling existed,
    /// cell for cell and style for style: no "more" line, no scroll hint.
    #[test]
    fn a_list_that_fits_renders_as_before() {
        let buffer = draw(&progress_app(Language::English, 33), 80, 33);

        let mut expected = Buffer::empty(Rect::new(0, 0, 80, 33));
        for (y, line) in FITTING_ENGLISH.iter().enumerate() {
            expected.set_string(0, y as u16, line.replace('~', NO_SAMPLE), Style::default());
        }
        let bold = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let early = Style::default().fg(Color::White);
        let dim = Style::default().fg(Color::DarkGray);
        // (row, columns, style) of each styled run: title, header, rule,
        // the focused 'e', the rest of the starter set, the locked letters
        // and the footer.
        let mut runs = vec![
            (0, 34..46, bold),
            (2, 8..63, bold),
            (3, 8..63, dim),
            (4, 8..71, bold),
        ];
        runs.extend((5..10).map(|y| (y, 8..63, early)));
        runs.extend((10..30).map(|y| (y, 8..63, dim)));
        runs.push((30, 18..62, dim));
        for (y, x, style) in runs {
            expected.set_style(Rect::new(x.start, y, x.end - x.start, 1), style);
        }
        assert_eq!(buffer, expected);
    }

    #[test]
    fn scroll_limits_follow_the_terminal_height() {
        // 33 rows hold English whole: title 2, header and rule 2, 26
        // letters, footer 3.
        assert_eq!(scroll_limits(&progress_app(Language::English, 33)), (26, 0));
        // One row fewer and the "more" line takes another: 24 letters show.
        assert_eq!(scroll_limits(&progress_app(Language::English, 32)), (24, 2));
        let letters = Language::German.unlock_order().len();
        assert_eq!(
            scroll_limits(&progress_app(Language::German, 30)),
            (22, letters - 22)
        );
        // However small the terminal, a page is a row or more and the last
        // offset still brings the last letter into view.
        for height in 0..15 {
            let (page, max) = scroll_limits(&progress_app(Language::German, height));
            assert!(page >= 1, "height {height}");
            assert_eq!(page + max, letters, "height {height}");
        }
    }

    /// German's letters on a 30-row terminal: the 25-row table holds the
    /// header, the rule, 22 letters and the "more" line.
    const SHORT: u16 = 30;
    const SHOWN: usize = 22;
    /// The Key column: the table starts at x 10 of 100, indented 2.
    const KEY_X: u16 = 12;

    #[test]
    fn a_long_list_shows_its_first_rows_and_how_many_are_below() {
        let order = Language::German.unlock_order();
        assert!(order.len() > SHOWN, "German must overflow {SHORT} rows");
        let buffer = draw(&progress_app(Language::German, SHORT), 100, SHORT);

        assert!(row_text(&buffer, 2).contains("Key    WPM"));
        assert!(row_text(&buffer, 3).contains("---  -----"));
        for (i, &key) in order[..SHOWN].iter().enumerate() {
            let y = 4 + i as u16;
            assert_eq!(buffer[(KEY_X, y)].symbol(), key.to_string(), "row {y}");
        }
        let below = order.len() - SHOWN;
        assert_eq!(row_text(&buffer, 26).trim(), format!("↓ {below} more"));
        assert_eq!(buffer[(KEY_X, 26)].fg, Color::DarkGray);
        let footer = row_text(&buffer, 27);
        assert!(footer.contains("[Up/Down] Scroll"), "{footer}");
        assert!(footer.contains("[Esc] Back to menu"), "{footer}");
    }

    #[test]
    fn end_scrolls_to_the_last_letter_and_up_comes_back_one() {
        let order = Language::German.unlock_order();
        let hidden = order.len() - SHOWN;
        let mut app = progress_app(Language::German, SHORT);
        press(&mut app, KeyCode::End);
        let end = draw(&app, 100, SHORT);

        // Header and rule stay put; the letters below them moved.
        assert!(row_text(&end, 2).contains("Key    WPM"));
        assert!(row_text(&end, 3).contains("---  -----"));
        assert_eq!(end[(KEY_X, 4)].symbol(), order[hidden].to_string());
        let last = order.last().expect("letters");
        assert_eq!(end[(KEY_X, 25)].symbol(), last.to_string());
        assert_eq!(row_text(&end, 26).trim(), format!("↑ {hidden} more"));

        // An offset past the end, which a resize to a taller terminal
        // leaves behind until the next key, draws the same last page.
        app.progress_scroll = usize::MAX;
        assert_eq!(draw(&app, 100, SHORT), end);

        press(&mut app, KeyCode::Up);
        let up = draw(&app, 100, SHORT);
        assert_eq!(up[(KEY_X, 4)].symbol(), order[hidden - 1].to_string());
        assert_eq!(
            row_text(&up, 26).trim(),
            format!("↑ {} more    ↓ 1 more", hidden - 1)
        );
    }

    #[test]
    fn any_terminal_height_draws_without_panicking() {
        let mut app = progress_app(Language::German, SHORT);
        for scroll in [0, 3, usize::MAX] {
            app.progress_scroll = scroll;
            for height in 1..=SHORT {
                draw(&app, 100, height);
            }
        }
    }
}
