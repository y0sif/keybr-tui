//! Semantic tree construction for the taria layer: the nodes published
//! alongside every rendered frame so an agent can perceive the app.
//!
//! This is a second, machine-facing View in the MVU split: pure over
//! [`App`] (state in, nodes out), it reads state and never writes it, so
//! the invariants are unit-testable. Ids are stable across frames and
//! exactly one node is focused on every screen. The reverse id lookups
//! ([`menu_item_index`], [`setting_index`]) live here too, so the act
//! router in [`crate::update`] can never drift from the published ids.
//!
//! The whole module is unix-only: taria's transport is a unix domain
//! socket, so `taria-ratatui` is a `cfg(unix)` dependency and `mod tree`
//! is declared under the same gate in `main.rs`.

use taria_ratatui::taria::id::IdSpace;
use taria_ratatui::taria::{Action, Node, Role};

use crate::app::{App, AppScreen, ErrorMode};
use crate::components::menu::MENU_ITEMS;
use crate::components::progress::tier_for;
use crate::components::settings::SETTINGS_COUNT;
use crate::engine::scheduler::{forced_extra_letters, UNLOCK_ORDER};

/// Id space for the main menu's rows: `menu-start-practice`, and so on.
///
/// One declaration for the spelling that `menu_item_id` writes and
/// `menu_item_index` reads back, so the pair cannot drift. Slugs contain the
/// separator themselves (`start-practice`); an id belongs to the space named
/// before its *first* separator, so that is the key and nothing else claims
/// it.
const MENU: IdSpace = IdSpace::new("menu");

/// Id space for the settings rows: `setting-target-wpm`, and so on.
const SETTING: IdSpace = IdSpace::new("setting");

/// Stable per-row slugs for the settings screen, index-aligned with the
/// rows rendered by `components::settings` and matched on by
/// `update::handle_settings_key`.
pub const SETTING_SLUGS: [&str; SETTINGS_COUNT] = [
    "target-wpm",
    "error-mode",
    "fragment-length",
    "alphabet-size",
    "focus-letter",
];

/// Lowercased, dash-separated form of a menu label ("Start Practice" ->
/// "start-practice"). Menu ids derive from the labels so a reordered or
/// renamed menu can't silently leave agents acting on the wrong item.
fn slug(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Node id for the menu item at `index` ("menu-start-practice", ...).
pub fn menu_item_id(index: usize) -> String {
    MENU.id(slug(MENU_ITEMS[index]))
}

/// Reverse lookup: which `MENU_ITEMS` index does this node id name?
pub fn menu_item_index(node_id: &str) -> Option<usize> {
    let rest = MENU.key(node_id)?;
    MENU_ITEMS.iter().position(|item| slug(item) == rest)
}

/// Node id for the settings row at `index` ("setting-target-wpm", ...).
pub fn setting_id(index: usize) -> String {
    SETTING.id(SETTING_SLUGS[index])
}

/// Reverse lookup: which settings row does this node id name?
pub fn setting_index(node_id: &str) -> Option<usize> {
    let rest = SETTING.key(node_id)?;
    SETTING_SLUGS.iter().position(|s| *s == rest)
}

/// Build the top-level semantic nodes for the current app state.
///
/// Invariant: exactly one node in the returned forest is focused, and only
/// nodes whose actions the act router honors advertise any — the typing
/// screen advertises none at all (typing arrives as `type_text` or raw
/// keys, and is scored exactly like a human keystroke).
pub fn build_nodes(app: &App) -> Vec<Node> {
    match app.screen {
        AppScreen::Menu => menu_nodes(app),
        AppScreen::Typing => typing_nodes(app),
        AppScreen::Progress => progress_nodes(app),
        AppScreen::Settings => settings_nodes(app),
    }
}

fn menu_nodes(app: &App) -> Vec<Node> {
    let items = MENU_ITEMS.iter().enumerate().map(|(i, &item)| {
        Node::new(menu_item_id(i), Role::ListItem)
            .label(item)
            .focused(i == app.menu_selection)
            .actions([Action::Select, Action::Activate])
    });
    vec![Node::new("menu", Role::List)
        .label("Main menu")
        .value(MENU_ITEMS.get(app.menu_selection).copied().unwrap_or(""))
        .children(items)]
}

fn typing_nodes(app: &App) -> Vec<Node> {
    let total = app.generated_text.chars().count();
    let mut nodes = vec![
        Node::new("target-text", Role::Text)
            .label("Target text")
            .value(app.generated_text.clone()),
        // The app tracks the cursor position, not the literal keys typed
        // (a wrong key advances the cursor in forgive mode without being
        // stored), so progress is reported as a position index.
        Node::new("typed", Role::Text)
            .label("Typed so far")
            .value(format!(
                "{} of {total} characters",
                app.cursor_pos.min(total)
            )),
        Node::new("stat-wpm", Role::Text)
            .label("WPM")
            .value(format!("{:.0}", app.lesson_wpm())),
        Node::new("stat-accuracy", Role::Text)
            .label("Accuracy")
            .value(format!("{:.0}%", app.lesson_accuracy())),
        // The dashboard's 26-tile heatmap. Its whole meaning is carried by
        // background colors an agent cannot see, which is what `Chart` is
        // for: publish the confidences the colors were made from.
        Node::new("key-heatmap", Role::Chart)
            .label("All keys — confidence per unlocked letter")
            .value(heatmap_value(app)),
    ];
    // Both dashboard rows below are conditional on screen, so they are
    // conditional here: a node published while its row is blank would tell
    // an agent about something the user cannot see.
    if app.daily_goal_minutes > 0 {
        let goal_secs = app.daily_goal_minutes.saturating_mul(60);
        let pct = if goal_secs == 0 {
            0.0
        } else {
            (app.today_seconds_practiced as f64 / goal_secs as f64).clamp(0.0, 1.0) * 100.0
        };
        nodes.push(
            Node::new("daily-goal", Role::ProgressBar)
                .label("Daily goal")
                .value(format!(
                    "{pct:.0}% of {} min ({} min practiced today)",
                    app.daily_goal_minutes,
                    app.today_seconds_practiced / 60
                )),
        );
    }
    // The "+ 'b' unlocked!" callout: on screen for one lesson and then gone
    // by itself. `Status` exists for exactly this — unpublished, an unlock
    // that happens between two `read_tree` calls is invisible, and the agent
    // reads the app as having done nothing.
    if let Some(ch) = app.last_lesson.as_ref().and_then(|r| r.newly_unlocked) {
        nodes.push(
            Node::new("unlock-notice", Role::Status)
                .label("Letter unlocked")
                .value(format!("+ '{ch}' unlocked!")),
        );
    }
    // Deliberately actionless: an agent takes the typing test the way a
    // human does. Typed text arrives as `type_text` and is scored one
    // keystroke at a time (see `update::apply_text`); Esc/Tab are raw keys.
    nodes.push(
        Node::new("typing", Role::TextInput)
            .label("Typing area — type_text is scored one keystroke at a time; Esc for menu")
            .focused(true),
    );
    nodes
}

/// The numbers behind the heatmap's colors: every unlocked letter with its
/// best confidence against the current target speed, plus how many letters
/// are still locked.
fn heatmap_value(app: &App) -> String {
    let mut parts: Vec<String> = Vec::new();
    for &key in UNLOCK_ORDER.iter() {
        if !app.scheduler.active_keys.contains(&key) {
            continue;
        }
        let conf = app
            .per_key_stats
            .get(&key)
            .map(|s| s.best_confidence(app.target_cpm))
            .unwrap_or(0.0);
        parts.push(format!("{key} {:.0}%", conf * 100.0));
    }
    let locked = UNLOCK_ORDER
        .len()
        .saturating_sub(app.scheduler.active_keys.len());
    if parts.is_empty() {
        return format!("no letters unlocked, {locked} locked");
    }
    format!("{}; {locked} locked", parts.join(", "))
}

fn progress_nodes(app: &App) -> Vec<Node> {
    let rows = UNLOCK_ORDER.iter().map(|&key| {
        let is_active = app.scheduler.active_keys.contains(&key);
        let stats = app.per_key_stats.get(&key);
        let tier = tier_for(is_active, stats, app.target_cpm);
        let fmt = |v: Option<f64>| match v {
            Some(v) => format!("{}", v.round() as i64),
            None => "—".to_string(),
        };
        let (wpm, attempts, errors) = match stats {
            Some(s) => (fmt(s.wpm()), s.attempts.to_string(), s.errors.to_string()),
            None => ("—".to_string(), "—".to_string(), "—".to_string()),
        };
        Node::new(format!("progress-key-{key}"), Role::Text)
            .label(key.to_string())
            .value(format!(
                "{wpm} wpm, {attempts} attempts, {errors} errors, {}",
                tier.label()
            ))
    });
    vec![Node::new("progress", Role::Pane)
        .label("Key progress")
        .value(format!("{} lessons completed", app.lesson_count))
        .focused(true)
        .action(Action::Dismiss)
        .children(rows)]
}

fn settings_nodes(app: &App) -> Vec<Node> {
    let mode_label = match app.error_mode {
        ErrorMode::ForgiveMistakes => "Forgive Mistakes",
        ErrorMode::StopOnError => "Stop On Error",
    };
    let focus_label = match app.manual_focus {
        Some(c) => c.to_ascii_uppercase().to_string(),
        None => "Auto".to_string(),
    };
    let rows: [(&str, String); SETTINGS_COUNT] = [
        ("Target WPM", app.target_wpm().to_string()),
        ("Error Mode", mode_label.to_string()),
        ("Fragment Length", app.fragment_length.to_string()),
        (
            "Alphabet size",
            format!("+{} letters", forced_extra_letters(app.alphabet_size)),
        ),
        ("Focus letter", focus_label),
    ];
    let children = rows.into_iter().enumerate().map(|(i, (label, value))| {
        Node::new(setting_id(i), Role::ListItem)
            .label(label)
            .value(value)
            .focused(i == app.settings_selection)
            .actions([
                Action::Select,
                Action::Custom("increase".into()),
                Action::Custom("decrease".into()),
            ])
    });
    vec![Node::new("settings", Role::List)
        .label("Settings")
        .children(children)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_focused(nodes: &[Node]) -> usize {
        nodes
            .iter()
            .map(|n| usize::from(n.focused) + count_focused(&n.children))
            .sum()
    }

    fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
        nodes.iter().find_map(|n| {
            (n.id.0 == id)
                .then_some(n)
                .or_else(|| find(&n.children, id))
        })
    }

    fn focused_id(nodes: &[Node]) -> Option<String> {
        nodes.iter().find_map(|n| {
            n.focused
                .then(|| n.id.0.clone())
                .or_else(|| focused_id(&n.children))
        })
    }

    fn all_actionless(nodes: &[Node]) -> bool {
        nodes
            .iter()
            .all(|n| n.actions.is_empty() && all_actionless(&n.children))
    }

    #[test]
    fn exactly_one_focused_on_every_screen() {
        let mut app = App::new();
        for (screen, expected) in [
            (AppScreen::Menu, "menu-start-practice"),
            (AppScreen::Typing, "typing"),
            (AppScreen::Progress, "progress"),
            (AppScreen::Settings, "setting-target-wpm"),
        ] {
            app.screen = screen;
            let nodes = build_nodes(&app);
            assert_eq!(count_focused(&nodes), 1, "screen {screen:?}");
            assert_eq!(focused_id(&nodes).as_deref(), Some(expected));
        }
    }

    #[test]
    fn menu_focus_follows_selection() {
        let mut app = App::new();
        app.menu_selection = 2;
        let nodes = build_nodes(&app);
        assert_eq!(count_focused(&nodes), 1);
        assert_eq!(focused_id(&nodes).as_deref(), Some("menu-settings"));
        assert_eq!(
            find(&nodes, "menu").unwrap().value.as_deref(),
            Some("Settings")
        );
    }

    #[test]
    fn menu_items_advertise_select_and_activate() {
        let app = App::new();
        let nodes = build_nodes(&app);
        for (i, &item) in MENU_ITEMS.iter().enumerate() {
            let node = find(&nodes, &menu_item_id(i)).expect("menu item node");
            assert_eq!(node.label.as_deref(), Some(item));
            assert_eq!(node.actions, vec![Action::Select, Action::Activate]);
        }
    }

    #[test]
    fn menu_and_setting_ids_roundtrip() {
        for i in 0..MENU_ITEMS.len() {
            assert_eq!(menu_item_index(&menu_item_id(i)), Some(i));
        }
        for i in 0..SETTINGS_COUNT {
            assert_eq!(setting_index(&setting_id(i)), Some(i));
        }
        assert_eq!(menu_item_index("menu-nope"), None);
        assert_eq!(menu_item_index("settings"), None);
        assert_eq!(setting_index("setting-nope"), None);
    }

    #[test]
    fn typing_screen_advertises_no_actions_anywhere() {
        let mut app = App::new();
        app.screen = AppScreen::Typing;
        let nodes = build_nodes(&app);
        assert!(
            all_actionless(&nodes),
            "typing goes through raw keys only; no node may advertise acts"
        );
        assert_eq!(focused_id(&nodes).as_deref(), Some("typing"));
    }

    #[test]
    fn typing_screen_exposes_target_text_and_position() {
        let mut app = App::new();
        app.screen = AppScreen::Typing;
        app.generated_text = "abc".to_string();
        app.cursor_pos = 1;
        let nodes = build_nodes(&app);
        assert_eq!(
            find(&nodes, "target-text").unwrap().value.as_deref(),
            Some("abc")
        );
        assert_eq!(
            find(&nodes, "typed").unwrap().value.as_deref(),
            Some("1 of 3 characters")
        );
        assert!(find(&nodes, "stat-wpm").unwrap().value.is_some());
        assert!(find(&nodes, "stat-accuracy").unwrap().value.is_some());
    }

    /// The three dashboard rows an agent could not see before: the heatmap
    /// (colour only), the daily-goal bar, and the unlock callout, which is
    /// on screen for one lesson and then gone by itself.
    #[test]
    fn typing_screen_publishes_the_dashboard_rows() {
        let mut app = App::new();
        app.screen = AppScreen::Typing;

        let nodes = build_nodes(&app);
        let heatmap = find(&nodes, "key-heatmap").expect("heatmap node");
        assert_eq!(heatmap.role, Role::Chart);
        let value = heatmap.value.as_deref().unwrap();
        assert!(value.contains("e "), "unlocked letters carry a confidence");
        assert!(value.contains("locked"));

        // The bar is published only while the user can see it.
        assert_eq!(
            find(&nodes, "daily-goal").unwrap().role,
            Role::ProgressBar,
            "the default config has a goal"
        );
        app.daily_goal_minutes = 0;
        assert!(find(&build_nodes(&app), "daily-goal").is_none());

        // And the callout only while it is on screen.
        assert!(find(&nodes, "unlock-notice").is_none());
        app.last_lesson = Some(crate::app::LessonResult {
            wpm: 40.0,
            accuracy: 95.0,
            newly_unlocked: Some('b'),
        });
        let with_notice = build_nodes(&app);
        let notice = find(&with_notice, "unlock-notice").expect("unlock notice");
        assert_eq!(notice.role, Role::Status);
        assert_eq!(notice.value.as_deref(), Some("+ 'b' unlocked!"));
    }

    #[test]
    fn progress_pane_is_dismissable_and_lists_all_keys() {
        let mut app = App::new();
        app.screen = AppScreen::Progress;
        let nodes = build_nodes(&app);
        let pane = find(&nodes, "progress").unwrap();
        assert_eq!(pane.actions, vec![Action::Dismiss]);
        assert_eq!(pane.children.len(), UNLOCK_ORDER.len());
        // 'e' heads the unlock order and is active from lesson one.
        let row = find(&nodes, "progress-key-e").unwrap();
        assert!(row.value.as_deref().unwrap().contains("Early"));
    }

    #[test]
    fn settings_rows_reflect_current_values() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        let nodes = build_nodes(&app);
        let expect = [
            ("setting-target-wpm", "35"),
            ("setting-error-mode", "Forgive Mistakes"),
            ("setting-fragment-length", "100"),
            ("setting-alphabet-size", "+0 letters"),
            ("setting-focus-letter", "Auto"),
        ];
        for (id, value) in expect {
            let node = find(&nodes, id).expect(id);
            assert_eq!(node.value.as_deref(), Some(value), "{id}");
            assert!(node.actions.contains(&Action::Select));
            assert!(node.actions.contains(&Action::Custom("increase".into())));
            assert!(node.actions.contains(&Action::Custom("decrease".into())));
        }
    }

    #[test]
    fn settings_focus_follows_selection() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 3;
        let nodes = build_nodes(&app);
        assert_eq!(count_focused(&nodes), 1);
        assert_eq!(focused_id(&nodes).as_deref(), Some("setting-alphabet-size"));
    }
}
