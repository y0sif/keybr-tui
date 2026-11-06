//! Agent input through the taria layer, unix only: `taria-ratatui` is a
//! `cfg(unix)` dependency, so none of this exists on Windows.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taria_ratatui::taria::{Action, AgentInput};
use taria_ratatui::to_crossterm_key;

use super::handle_key;
use super::typing::handle_typed_char;
use crate::app::{App, AppScreen};
use crate::tree::{menu_item_index, setting_index};

/// What the app did with one agent input.
///
/// The taria layer acks every input `Delivered` as the event loop dequeues
/// it, which only says it was dequeued. [`Ignored`](Applied::Ignored) is the
/// follow-up main sends so an agent waiting on an effect stops waiting; see
/// `main::drain_agent_input`. Before this existed every arm below simply
/// returned, and an agent that acted on the wrong screen waited out a
/// timeout to learn nothing had happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The input reached a handler that could act on it.
    Handled,
    /// The app looked at the input and deliberately did nothing.
    Ignored,
}

/// Apply one agent input received through the taria layer.
///
/// Acts are macros over the existing key transitions, nothing more: a
/// selection id resolves to the same index the arrow keys reach, then the
/// act lowers into the exact [`KeyEvent`] a human would press, routed
/// through the same `handle_key` path. An agent therefore cannot reach any
/// state a keyboard user cannot. Pure like [`update`](super::update): no I/O, only the
/// deferred save flags.
///
/// # Typed text goes to the typing screen, never through `handle_key`
///
/// [`AgentInput::Text`] is typing, and in this app the surface that takes
/// typing is the lesson: [`apply_text`] feeds it one scored keystroke at a
/// time. It must not be lowered through [`handle_key`] the way
/// [`AgentInput::Key`] is, because this app's screens bind bare letters as
/// commands. `type_text("quick")` on the menu screen would meet the menu's
/// `q` binding, save stats and exit the app before its second character; a
/// tab inside a string would meet the typing screen's Tab binding and flip
/// the error mode mid-lesson, silently rewriting the user's config file.
/// Neither is something the agent asked for, and the tree changing (or the
/// process ending) would have read as success.
///
/// [`AgentInput::Key`] keeps the raw lowering on purpose. A key is a
/// keypress and is meant to reach the bindings wherever focus is; that is
/// the whole difference between the two inputs.
pub fn apply_agent_input(app: &mut App, input: AgentInput) -> Applied {
    match input {
        AgentInput::Act { node, action, .. } => apply_act(app, node.0.as_str(), action),
        AgentInput::Key { key, .. } => match to_crossterm_key(&key) {
            Some(key) => {
                handle_key(app, key);
                Applied::Handled
            }
            // Either the shared grammar rejected the string or it named a key
            // this adapter cannot lower. Nothing was pressed, so say so.
            None => Applied::Ignored,
        },
        AgentInput::Text { text, .. } => apply_text(app, &text),
        // An input kind taria grew after this build. It carries nothing this
        // app could act on, so it is reported like every other input the app
        // looked at and did nothing with.
        _ => Applied::Ignored,
    }
}

/// Type `text` into the lesson, one scored keystroke per character.
///
/// This is the typing tutor's answer to "where does the app put typing":
/// there is no text field, so a character does not land in a buffer, it is
/// scored against the target character under the cursor exactly as if a
/// human had pressed that key. Wrong characters count as errors, and per-key
/// reaction times are recorded, which is the point of the app.
///
/// Three rules follow from that, and none of them are the text-field rules:
///
/// * **Only on the typing screen.** Every other screen accepts no typing at
///   all, so text sent there is [`Ignored`](Applied::Ignored) rather than
///   fed to a key handler that reads letters as commands.
/// * **Control characters are skipped, not lowered.** `taria_ratatui`'s
///   [`text_to_keys`](taria_ratatui::text_to_keys) maps `'\n'` to Enter and
///   `'\t'` to Tab, which is right for a text field and wrong here: Tab is
///   this screen's error-mode toggle. A lesson holds letters and spaces, so
///   a control character in the payload is a keystroke this surface does not
///   score, and skipping it is what a human's Enter does mid-lesson.
/// * **One call types at most one lesson.** Finishing a lesson immediately
///   generates the next one ([`App::finish_lesson`]), so the characters
///   after the last one would be scored against text the agent has never
///   read. It stops at the boundary instead and the agent reads the new
///   lesson before typing again.
fn apply_text(app: &mut App, text: &str) -> Applied {
    if app.screen != AppScreen::Typing {
        return Applied::Ignored;
    }
    let lesson = app.lesson_count;
    let mut typed = false;
    for ch in text.chars() {
        // `finish_lesson` bumps the count and swaps in text the agent has
        // not seen; the screen guard is belt and braces.
        if app.screen != AppScreen::Typing || app.lesson_count != lesson {
            break;
        }
        if ch.is_control() {
            continue;
        }
        handle_typed_char(app, ch);
        typed = true;
    }
    if typed {
        Applied::Handled
    } else {
        Applied::Ignored
    }
}

/// A plain keypress, as synthesized for lowering acts into key handling.
fn plain(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn apply_act(app: &mut App, node: &str, action: Action) -> Applied {
    // Only the current screen's advertised acts are honored; the published
    // tree (see `crate::tree`) advertises none elsewhere, and behavior
    // must agree with advertisement. The typing screen advertises no
    // actions at all — typing goes through `Text` and the raw-key fallback.
    match app.screen {
        AppScreen::Menu => apply_menu_act(app, node, action),
        AppScreen::Typing => Applied::Ignored,
        AppScreen::Progress => apply_progress_act(app, node, action),
        AppScreen::Settings => apply_settings_act(app, node, action),
    }
}

fn apply_menu_act(app: &mut App, node: &str, action: Action) -> Applied {
    let Some(index) = menu_item_index(node) else {
        return Applied::Ignored;
    };
    match action {
        Action::Select => app.menu_selection = index,
        Action::Activate => {
            app.menu_selection = index;
            handle_key(app, plain(KeyCode::Enter));
        }
        _ => return Applied::Ignored,
    }
    Applied::Handled
}

fn apply_progress_act(app: &mut App, node: &str, action: Action) -> Applied {
    if node == "progress" && action == Action::Dismiss {
        handle_key(app, plain(KeyCode::Esc));
        return Applied::Handled;
    }
    Applied::Ignored
}

fn apply_settings_act(app: &mut App, node: &str, action: Action) -> Applied {
    let Some(index) = setting_index(node) else {
        return Applied::Ignored;
    };
    match action {
        Action::Select => app.settings_selection = index,
        Action::Custom(name) if name == "increase" => {
            app.settings_selection = index;
            handle_key(app, plain(KeyCode::Right));
        }
        Action::Custom(name) if name == "decrease" => {
            app.settings_selection = index;
            handle_key(app, plain(KeyCode::Left));
        }
        _ => return Applied::Ignored,
    }
    Applied::Handled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::screens::settings::LANGUAGE_ROW;
    use crate::update::test_support::make_test_app;
    use taria_ratatui::taria::NodeId;

    fn act(node: &str, action: Action) -> AgentInput {
        AgentInput::act(NodeId(node.into()), action, None)
    }

    fn agent_key(key: &str) -> AgentInput {
        AgentInput::key(key)
    }

    fn agent_text(text: &str) -> AgentInput {
        AgentInput::text(text)
    }

    #[test]
    fn agent_select_moves_menu_selection() {
        let mut app = App::new();
        assert_eq!(app.screen, AppScreen::Menu);
        apply_agent_input(&mut app, act("menu-settings", Action::Select));
        assert_eq!(app.menu_selection, 2);
        assert_eq!(app.screen, AppScreen::Menu, "select alone must not open");
    }

    #[test]
    fn agent_activate_opens_menu_item() {
        let mut app = App::new();
        apply_agent_input(&mut app, act("menu-view-progress", Action::Activate));
        assert_eq!(app.screen, AppScreen::Progress);

        let mut app = App::new();
        apply_agent_input(&mut app, act("menu-start-practice", Action::Activate));
        assert_eq!(app.screen, AppScreen::Typing);
    }

    #[test]
    fn agent_activate_quit_raises_save_flag_like_a_human_quit() {
        let mut app = App::new();
        apply_agent_input(&mut app, act("menu-quit", Action::Activate));
        assert!(!app.running);
        assert!(app.pending_stats_save);
    }

    #[test]
    fn agent_act_on_unknown_menu_node_is_ignored() {
        let mut app = App::new();
        let applied = apply_agent_input(&mut app, act("menu-nope", Action::Activate));
        assert_eq!(app.screen, AppScreen::Menu);
        assert_eq!(app.menu_selection, 0);
        assert!(app.running);
        // And the agent is told, rather than waiting out a timeout for an
        // effect that was never going to come.
        assert_eq!(applied, Applied::Ignored);
    }

    #[test]
    fn agent_act_the_router_does_not_handle_is_ignored() {
        // Every early return in the act router: an action a row does not
        // advertise, an unknown settings row, the progress pane's non-act.
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        assert_eq!(
            apply_agent_input(&mut app, act("setting-target-wpm", Action::Toggle)),
            Applied::Ignored
        );
        assert_eq!(
            apply_agent_input(&mut app, act("setting-nope", Action::Select)),
            Applied::Ignored
        );
        app.screen = AppScreen::Progress;
        assert_eq!(
            apply_agent_input(&mut app, act("progress-key-e", Action::Activate)),
            Applied::Ignored
        );
        app.screen = AppScreen::Menu;
        assert_eq!(
            apply_agent_input(&mut app, act("menu-quit", Action::Toggle)),
            Applied::Ignored
        );
        assert!(app.running, "an ignored act must not quit");
    }

    #[test]
    fn agent_settings_increase_and_decrease_adjust_value() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        let initial_wpm = app.target_wpm();

        apply_agent_input(
            &mut app,
            act("setting-target-wpm", Action::Custom("increase".into())),
        );
        assert_eq!(app.target_wpm(), initial_wpm + 5);
        assert!(app.pending_config_save, "adjustment must request a save");

        apply_agent_input(
            &mut app,
            act("setting-target-wpm", Action::Custom("decrease".into())),
        );
        assert_eq!(app.target_wpm(), initial_wpm);
    }

    #[test]
    fn agent_settings_select_moves_selection_only() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        apply_agent_input(&mut app, act("setting-focus-letter", Action::Select));
        assert_eq!(app.settings_selection, 4);
        assert_eq!(app.manual_focus, None, "select alone must not adjust");
    }

    #[test]
    fn agent_settings_increase_targets_the_named_row() {
        // The act names its row explicitly, regardless of the current
        // selection — it lowers to select-then-Right.
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 0;
        apply_agent_input(
            &mut app,
            act("setting-fragment-length", Action::Custom("increase".into())),
        );
        assert_eq!(app.fragment_length, 110);
        assert_eq!(app.settings_selection, 2);
        assert_eq!(app.target_wpm(), 35, "other rows stay untouched");
    }

    #[test]
    fn agent_settings_language_acts_request_a_switch() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        assert_eq!(
            apply_agent_input(
                &mut app,
                act("setting-language", Action::Custom("increase".into()))
            ),
            Applied::Handled
        );
        assert_eq!(app.settings_selection, LANGUAGE_ROW);
        assert_eq!(app.pending_language_switch, Some(Language::ALL[1]));
        assert_eq!(app.language, Language::English);

        apply_agent_input(
            &mut app,
            act("setting-language", Action::Custom("decrease".into())),
        );
        assert_eq!(app.pending_language_switch, None);
    }

    #[test]
    fn agent_progress_dismiss_returns_to_menu() {
        let mut app = App::new();
        app.screen = AppScreen::Progress;
        apply_agent_input(&mut app, act("progress", Action::Dismiss));
        assert_eq!(app.screen, AppScreen::Menu);
    }

    #[test]
    fn typing_screen_refuses_all_acts() {
        // The typing screen advertises no actions; acts against it (or any
        // stale id from another screen) must be complete no-ops.
        let mut app = make_test_app("abc");
        for input in [
            act("typing", Action::Activate),
            act("typing", Action::Select),
            act("typing", Action::SetValue),
            act("menu-settings", Action::Activate),
            act("setting-target-wpm", Action::Custom("increase".into())),
            act("progress", Action::Dismiss),
        ] {
            assert_eq!(apply_agent_input(&mut app, input), Applied::Ignored);
        }
        assert_eq!(app.screen, AppScreen::Typing);
        assert_eq!(app.cursor_pos, 0);
        assert_eq!(app.target_wpm(), 35);
        assert!(app.running);
    }

    #[test]
    fn agent_key_fallback_types_exactly_like_a_human_keystroke() {
        let mut app = make_test_app("abc");
        apply_agent_input(&mut app, agent_key("a"));
        assert_eq!(app.cursor_pos, 1);
        assert!(app.first_attempt_correct.contains(&0));
        assert!(!app.recovered_positions.contains(&0));

        // A wrong key registers as an error, same as the human path.
        apply_agent_input(&mut app, agent_key("x"));
        assert_eq!(app.cursor_pos, 2, "forgive mode advances past the error");
        assert!(app.error_positions.contains(&1));
        assert!(app.ever_error_positions.contains(&1));
    }

    #[test]
    fn agent_key_fallback_esc_leaves_typing_screen() {
        let mut app = make_test_app("abc");
        apply_agent_input(&mut app, agent_key("esc"));
        assert_eq!(app.screen, AppScreen::Menu);
        assert!(app.pending_stats_save);
    }

    #[test]
    fn agent_unparseable_key_is_ignored() {
        let mut app = make_test_app("abc");
        let applied = apply_agent_input(&mut app, agent_key("not-a-key"));
        assert_eq!(app.cursor_pos, 0);
        assert_eq!(app.screen, AppScreen::Typing);
        assert_eq!(applied, Applied::Ignored);
    }

    // --- Typed text (`type_text`) ---

    #[test]
    fn agent_text_is_scored_one_keystroke_at_a_time() {
        let mut app = make_test_app("abc");
        assert_eq!(
            apply_agent_input(&mut app, agent_text("ab")),
            Applied::Handled
        );
        assert_eq!(app.cursor_pos, 2);
        assert!(app.first_attempt_correct.contains(&0));
        assert!(app.first_attempt_correct.contains(&1));

        // A wrong character in the payload is an error, exactly as it is
        // through the key path. This is the whole app: text is not stored,
        // it is graded.
        let mut app = make_test_app("abc");
        assert_eq!(
            apply_agent_input(&mut app, agent_text("ax")),
            Applied::Handled
        );
        assert!(app.error_positions.contains(&1));
        assert_eq!(app.lesson_errors, 1);
    }

    /// The regression this app exists to prove. Lowered through
    /// `handle_key`, `type_text("quick")` would meet the menu's `q`
    /// binding, raise the save flag and end the process at its first
    /// character, and the bridge would have reported it a success.
    #[test]
    fn agent_text_on_the_menu_does_not_trip_the_quit_binding() {
        let mut app = App::new();
        assert_eq!(app.screen, AppScreen::Menu);
        assert_eq!(
            apply_agent_input(&mut app, agent_text("quick")),
            Applied::Ignored
        );
        assert!(app.running, "typed text must never reach the q binding");
        assert!(!app.pending_stats_save);
        assert_eq!(app.screen, AppScreen::Menu);
        assert_eq!(app.menu_selection, 0);
    }

    #[test]
    fn agent_text_is_ignored_on_every_screen_that_takes_no_typing() {
        for screen in [AppScreen::Menu, AppScreen::Progress, AppScreen::Settings] {
            let mut app = App::new();
            app.screen = screen;
            assert_eq!(
                apply_agent_input(&mut app, agent_text("hello")),
                Applied::Ignored,
                "screen {screen:?}"
            );
            assert_eq!(app.screen, screen, "screen {screen:?} must not move");
        }
    }

    /// A tab inside typed text must not reach the typing screen's Tab
    /// binding, which toggles the error mode and rewrites the config file.
    #[test]
    fn agent_text_skips_control_characters_instead_of_lowering_them() {
        let mut app = make_test_app("abc");
        let before = app.error_mode;
        assert_eq!(
            apply_agent_input(&mut app, agent_text("a\t\nb")),
            Applied::Handled
        );
        assert_eq!(app.cursor_pos, 2, "only 'a' and 'b' are keystrokes here");
        assert_eq!(app.error_mode, before, "a tab must not toggle the mode");
        assert!(!app.pending_config_save);
    }

    /// Finishing a lesson immediately generates the next one, so the tail of
    /// an over-long payload would be graded against text the agent has never
    /// read. It stops at the boundary instead.
    #[test]
    fn agent_text_stops_at_the_lesson_boundary() {
        let mut app = make_test_app("ab");
        let applied = apply_agent_input(&mut app, agent_text("abcdefgh"));
        assert_eq!(applied, Applied::Handled);
        assert_eq!(app.lesson_count, 1, "the lesson finished");
        assert_eq!(
            app.cursor_pos, 0,
            "the next lesson is untouched by the leftover characters"
        );
    }

    #[test]
    fn agent_empty_text_is_ignored() {
        let mut app = make_test_app("abc");
        assert_eq!(
            apply_agent_input(&mut app, agent_text("")),
            Applied::Ignored
        );
        assert_eq!(app.cursor_pos, 0);
    }
}
