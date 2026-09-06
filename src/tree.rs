//! Semantic tree construction for the taria layer: the nodes published
//! alongside every rendered frame so an agent can perceive the app.
//!
//! This is a second, machine-facing View in the MVU split: pure over
//! [`App`] (state in, nodes out), it reads state and never writes it, so
//! the invariants are unit-testable. Ids are stable across frames and
//! exactly one node is focused on every screen. The reverse id lookups
//! ([`menu_item_index`], [`setting_index`]) live here too, so the act
//! router in [`crate::update`] can never drift from the published ids.

use taria_ratatui::taria::{Action, Node, Role};

use crate::app::{App, AppScreen, ErrorMode};
use crate::components::menu::MENU_ITEMS;
use crate::components::progress::tier_for;
use crate::components::settings::SETTINGS_COUNT;
use crate::engine::scheduler::{forced_extra_letters, UNLOCK_ORDER};

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
    format!("menu-{}", slug(MENU_ITEMS[index]))
}

/// Reverse lookup: which `MENU_ITEMS` index does this node id name?
pub fn menu_item_index(node_id: &str) -> Option<usize> {
    let rest = node_id.strip_prefix("menu-")?;
    MENU_ITEMS.iter().position(|item| slug(item) == rest)
}

/// Node id for the settings row at `index` ("setting-target-wpm", ...).
pub fn setting_id(index: usize) -> String {
    format!("setting-{}", SETTING_SLUGS[index])
}

/// Reverse lookup: which settings row does this node id name?
pub fn setting_index(node_id: &str) -> Option<usize> {
    let rest = node_id.strip_prefix("setting-")?;
    SETTING_SLUGS.iter().position(|s| *s == rest)
}

/// Build the top-level semantic nodes for the current app state.
///
/// Invariant: exactly one node in the returned forest is focused, and only
/// nodes whose actions the act router honors advertise any — the typing
/// screen advertises none at all (typing goes through the raw-key
/// fallback, exactly like a human keystroke).
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
    vec![
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
        // Deliberately actionless: an agent takes the typing test the way
        // a human does, one raw key at a time. Esc/Tab shortcuts are raw
        // keys too.
        Node::new("typing", Role::TextInput)
            .label("Typing area — send raw keys to type; Esc for menu")
            .focused(true),
    ]
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
