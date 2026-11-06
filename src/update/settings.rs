//! Settings screen key handling: row navigation and each row's value
//! steps, including the focus pins and the language switch request.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::request_config_save;
use crate::app::{App, AppScreen, ErrorMode};
use crate::engine::filter::FocusRule;
use crate::engine::Language;
use crate::screens::settings::{LANGUAGE_ROW, SETTINGS_COUNT};

/// Index of the current pin inside a `[none] + values` cycle, where 0 is
/// the "none" slot, plus the index the step lands on. Shared by both
/// focus rows so they wrap identically.
fn cycle_index(current: Option<usize>, len: usize, forward: bool) -> usize {
    let total = len + 1;
    let current = current.map_or(0, |i| i + 1);
    if forward {
        (current + 1) % total
    } else {
        (current + total - 1) % total
    }
}

/// Step the manual focus pin through `[Auto] + active_keys` (unlock
/// order), wrapping at both ends. `forward` is the Right key. A stale
/// pin (letter no longer in `active_keys`) counts as Auto, matching
/// how `App::effective_focus` treats it, and so does a pattern pin:
/// stepping off Auto here therefore replaces it, which is how the two
/// rows stay mutually exclusive without either checking the other.
fn cycle_manual_focus(app: &mut App, forward: bool) {
    let keys = &app.scheduler.active_keys;
    let current = app.manual_focus.and_then(|rule| match rule {
        FocusRule::Key(c) => keys.iter().position(|&k| k == c),
        _ => None,
    });
    let next = cycle_index(current, keys.len(), forward);
    let picked = (next > 0).then(|| FocusRule::Key(keys[next - 1]));
    pin_focus(app, picked);
}

/// Set the manual focus pin from Settings. The user's pick replaces
/// whatever pin config.toml held, including one kept aside for another
/// language (see `App::set_manual_focus_from_config`).
fn pin_focus(app: &mut App, rule: Option<FocusRule>) {
    app.manual_focus = rule;
    app.foreign_focus_letter = None;
    app.foreign_focus_pattern = None;
}

/// Step the manual focus pin through `[Off] + available_patterns()`,
/// wrapping at both ends. `forward` is the Right key.
///
/// The mirror of `cycle_manual_focus`: a letter pin counts as Off here,
/// so stepping off Off replaces it. Both write the one `manual_focus`
/// field, which is what makes a letter pin and a pattern pin impossible
/// to hold at once.
///
/// With no drill available (an early profile, too few letters unlocked)
/// there is nothing to step to, so any existing pin is left alone rather
/// than cleared by a row that cannot offer a replacement. `false` says
/// exactly that happened, so the caller can skip the config save for a
/// row that changed nothing.
fn cycle_manual_pattern(app: &mut App, forward: bool) -> bool {
    let patterns = app.available_patterns();
    if patterns.is_empty() {
        return false;
    }
    let current = app
        .manual_focus
        .and_then(|rule| patterns.iter().position(|&p| p == rule));
    let next = cycle_index(current, patterns.len(), forward);
    let picked = (next > 0).then(|| patterns[next - 1]);
    pin_focus(app, picked);
    true
}

/// Request a switch to the next (`forward`, the Right key) or previous
/// practice language in `Language::ALL`, wrapping at both ends.
///
/// Only records the request: a switch saves this language's stats and
/// loads the new one's, so `App::flush_pending_saves` performs it. Steps
/// from the language already requested, if any, so two steps queued
/// before a flush (agent inputs are drained twice per loop iteration)
/// land two languages on. Stepping back to the current language leaves
/// nothing pending.
fn step_language(app: &mut App, forward: bool) {
    let all = Language::ALL;
    let from = app.pending_language_switch.unwrap_or(app.language);
    let current = all.iter().position(|&l| l == from).unwrap_or(0);
    let next = if forward {
        (current + 1) % all.len()
    } else {
        (current + all.len() - 1) % all.len()
    };
    app.pending_language_switch = Some(all[next]).filter(|&l| l != app.language);
}

pub(super) fn handle_settings_key(app: &mut App, key: KeyEvent) {
    use KeyCode::*;

    match key.code {
        Esc => {
            request_config_save(app);
            app.screen = AppScreen::Menu;
        }
        Up => {
            if app.settings_selection > 0 {
                app.settings_selection -= 1;
            } else {
                app.settings_selection = SETTINGS_COUNT - 1;
            }
        }
        Down => {
            app.settings_selection = (app.settings_selection + 1) % SETTINGS_COUNT;
        }
        Left => {
            // Each arm reports whether it actually changed a setting, so a
            // row with nothing to offer does not mark the config dirty.
            let changed = match app.settings_selection {
                0 => {
                    // Decrease target WPM
                    let new_wpm = app.target_wpm().saturating_sub(5).max(10);
                    app.set_target_wpm(new_wpm);
                    true
                }
                1 => {
                    // Toggle error mode
                    app.error_mode = match app.error_mode {
                        ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                        ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                    };
                    true
                }
                2 => {
                    // Decrease fragment length
                    app.fragment_length = app.fragment_length.saturating_sub(10).max(20);
                    true
                }
                3 => {
                    // Decrease alphabet size (one forced letter per 0.05 step).
                    // Round to 2 decimals so repeated steps don't drift.
                    // This moves the scheduler's setting, not `active_keys`:
                    // the letters it forces are added by the next
                    // `run_scheduler_update`, which refreshes the drill list.
                    app.alphabet_size = ((app.alphabet_size - 0.05).max(0.0) * 100.0).round() / 100.0;
                    app.scheduler.alphabet_size = app.alphabet_size;
                    true
                }
                4 => {
                    // Step focus letter backward (Auto wraps to last unlocked).
                    cycle_manual_focus(app, false);
                    true
                }
                5 => {
                    // Step focus pattern backward (Off wraps to the last drill).
                    cycle_manual_pattern(app, false)
                }
                LANGUAGE_ROW => {
                    // Request the previous language (the first wraps to the
                    // last). No save here: the App the switch builds
                    // requests its own config save.
                    step_language(app, false);
                    false
                }
                _ => false,
            };
            if changed {
                request_config_save(app);
            }
        }
        Right => {
            // Mirror of Left: only a real change requests a config save.
            let changed = match app.settings_selection {
                0 => {
                    // Increase target WPM
                    let new_wpm = (app.target_wpm() + 5).min(200);
                    app.set_target_wpm(new_wpm);
                    true
                }
                1 => {
                    // Toggle error mode
                    app.error_mode = match app.error_mode {
                        ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                        ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                    };
                    true
                }
                2 => {
                    // Increase fragment length
                    app.fragment_length = (app.fragment_length + 10).min(500);
                    true
                }
                3 => {
                    // Increase alphabet size (one forced letter per 0.05 step).
                    // See the Left arm: `active_keys` moves at the next
                    // `run_scheduler_update`, not here.
                    app.alphabet_size = ((app.alphabet_size + 0.05).min(1.0) * 100.0).round() / 100.0;
                    app.scheduler.alphabet_size = app.alphabet_size;
                    true
                }
                4 => {
                    // Step focus letter forward (last unlocked wraps to Auto).
                    cycle_manual_focus(app, true);
                    true
                }
                5 => {
                    // Step focus pattern forward (the last drill wraps to Off).
                    cycle_manual_pattern(app, true)
                }
                LANGUAGE_ROW => {
                    // Request the next language (the last wraps to the
                    // first). See the Left arm for why nothing is saved.
                    step_language(app, true);
                    false
                }
                _ => false,
            };
            if changed {
                request_config_save(app);
            }
        }
        Enter
            // Toggle error mode on Enter when selected
            if app.settings_selection == 1 => {
                app.error_mode = match app.error_mode {
                    ErrorMode::ForgiveMistakes => ErrorMode::StopOnError,
                    ErrorMode::StopOnError => ErrorMode::ForgiveMistakes,
                };
                request_config_save(app);
            }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AppEvent;
    use crate::update::test_support::make_key;
    use crate::update::update;

    #[test]
    fn settings_esc_goes_to_menu() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Esc)));
        assert_eq!(app.screen, AppScreen::Menu);
    }

    #[test]
    fn settings_adjusts_wpm() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 0;
        let initial_wpm = app.target_wpm();
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.target_wpm(), initial_wpm + 5);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.target_wpm(), initial_wpm);
    }

    #[test]
    fn settings_toggles_error_mode() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 1;
        app.error_mode = ErrorMode::ForgiveMistakes;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.error_mode, ErrorMode::StopOnError);
    }

    #[test]
    fn settings_adjusts_fragment_length() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 2;
        let initial = app.fragment_length;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.fragment_length, initial + 10);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.fragment_length, initial);
    }

    #[test]
    fn settings_adjusts_alphabet_size() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 3;
        assert_eq!(app.alphabet_size, 0.0);

        // One step right: +0.05, mirrored onto the scheduler.
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.alphabet_size, 0.05);
        assert_eq!(app.scheduler.alphabet_size, 0.05);

        // Back left to the floor — and never below 0.0.
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.alphabet_size, 0.0);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.alphabet_size, 0.0);
        assert_eq!(app.scheduler.alphabet_size, 0.0);

        // Saturates at 1.0 (20 steps from 0.0, then a few more).
        for _ in 0..25 {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        }
        assert_eq!(app.alphabet_size, 1.0);
        assert_eq!(app.scheduler.alphabet_size, 1.0);

        // And back down to the floor without drift.
        for _ in 0..25 {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        }
        assert_eq!(app.alphabet_size, 0.0);
    }

    #[test]
    fn settings_focus_right_cycles_and_wraps_to_auto() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 4;
        assert_eq!(app.manual_focus, None);

        // Auto -> first unlocked letter ('e' heads the unlock order).
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('e')));
        assert!(app.pending_config_save, "pin change must request a save");

        // Stepping through the remaining unlocked letters lands back on Auto.
        let remaining = app.scheduler.active_keys.len();
        for _ in 0..remaining {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        }
        assert_eq!(app.manual_focus, None);
    }

    #[test]
    fn settings_focus_left_from_auto_wraps_to_last_letter() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 4;
        assert_eq!(app.manual_focus, None);

        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(
            app.manual_focus,
            app.scheduler
                .active_keys
                .last()
                .copied()
                .map(FocusRule::Key)
        );
        assert!(app.pending_config_save);

        // And one step forward returns to Auto.
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, None);
    }

    // --- Focus pattern row (combination drills) ---

    /// A profile with the whole alphabet unlocked, which is what makes
    /// the combination drills reachable, parked on the pattern row.
    ///
    /// Writing `active_keys` by hand skips `App::run_scheduler_update`,
    /// so the derived drill list is refreshed the same way that path
    /// does it.
    fn app_on_pattern_row() -> App {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 5;
        app.scheduler.active_keys = ('a'..='z').collect();
        app.refresh_available_patterns();
        app
    }

    #[test]
    fn settings_pattern_right_cycles_and_wraps_to_off() {
        let mut app = app_on_pattern_row();
        let patterns = app.available_patterns().to_vec();
        assert!(!patterns.is_empty(), "full alphabet must offer drills");
        assert_eq!(app.manual_focus, None);

        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, Some(patterns[0]));
        assert!(app.pending_config_save, "pin change must request a save");

        // Stepping through the rest lands back on Off.
        for _ in 0..patterns.len() {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        }
        assert_eq!(app.manual_focus, None);
    }

    #[test]
    fn settings_pattern_left_from_off_wraps_to_last_drill() {
        let mut app = app_on_pattern_row();
        let patterns = app.available_patterns().to_vec();

        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.manual_focus, patterns.last().copied());
        assert!(app.pending_config_save);

        // Walking back the other way returns to Off.
        for _ in 0..patterns.len() {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        }
        assert_eq!(app.manual_focus, None);
    }

    #[test]
    fn choosing_a_pattern_clears_a_letter_pin() {
        let mut app = app_on_pattern_row();
        app.manual_focus = Some(FocusRule::Key('r'));

        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        let pinned = app.manual_focus.expect("a drill must now be pinned");
        assert!(
            !matches!(pinned, FocusRule::Key(_)),
            "the letter pin must be gone, got {pinned:?}"
        );
    }

    #[test]
    fn choosing_a_letter_clears_a_pattern_pin() {
        let mut app = app_on_pattern_row();
        app.manual_focus = Some(FocusRule::Contains("cr"));
        app.settings_selection = 4;

        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        // A pattern reads as Auto on the letter row, so one step right
        // lands on the first unlocked letter and the pattern is gone.
        assert_eq!(
            app.manual_focus,
            app.scheduler
                .active_keys
                .first()
                .copied()
                .map(FocusRule::Key)
        );
    }

    #[test]
    fn pattern_row_is_inert_when_no_drill_is_available() {
        // Too few letters to spell any drill: the row has nothing to
        // offer, and must not clear an existing letter pin either.
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 5;
        app.scheduler.active_keys = vec!['e', 'n', 'i', 'a'];
        app.refresh_available_patterns();
        assert!(app.available_patterns().is_empty());

        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, None);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.manual_focus, None);

        app.manual_focus = Some(FocusRule::Key('r'));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('r')));

        // Nothing changed, so nothing is owed to the config file.
        assert!(
            !app.pending_config_save,
            "a row with nothing to offer must not mark the config dirty"
        );
    }

    #[test]
    fn picking_a_pin_replaces_one_kept_for_another_language() {
        // Turkish keeps English pins it cannot use in the config file.
        // Its pattern row offers nothing and leaves them alone; picking a
        // letter is a new pin and replaces them.
        let mut app = App::new_with_language(
            35,
            ErrorMode::ForgiveMistakes,
            None,
            crate::engine::Language::Turkish,
        );
        app.set_manual_focus_from_config(Some("-tion"), Some('q'));
        app.screen = AppScreen::Settings;
        app.settings_selection = 5;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        let cfg = app.to_config();
        assert_eq!(cfg.focus_pattern.as_deref(), Some("-tion"));
        assert_eq!(cfg.focus_letter, Some('q'));

        app.settings_selection = 4;
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('a')));
        let cfg = app.to_config();
        assert_eq!(cfg.focus_letter, Some('a'));
        assert_eq!(cfg.focus_pattern, None);
    }

    // --- Language row ---

    /// An App practicing `language`, parked on the Settings language row.
    fn app_on_language_row(language: Language) -> App {
        let mut app = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, language);
        app.screen = AppScreen::Settings;
        app.settings_selection = LANGUAGE_ROW;
        app
    }

    #[test]
    fn language_row_is_the_last_row() {
        // Settings starts on row 0 at launch, so the first visit never
        // opens on the Language row.
        assert_eq!(LANGUAGE_ROW, SETTINGS_COUNT - 1);
        assert_eq!(App::new().settings_selection, 0);
    }

    #[test]
    fn language_right_requests_the_next_language_and_wraps() {
        let first = Language::ALL[0];
        let last = *Language::ALL.last().unwrap();

        let mut app = app_on_language_row(first);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.pending_language_switch, Some(Language::ALL[1]));
        assert_eq!(app.language, first, "update only records the switch");

        let mut app = app_on_language_row(last);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.pending_language_switch, Some(first));
        assert_eq!(app.language, last);
    }

    #[test]
    fn language_left_requests_the_previous_language_and_wraps() {
        let first = Language::ALL[0];
        let last = *Language::ALL.last().unwrap();

        let mut app = app_on_language_row(first);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.pending_language_switch, Some(last));
        assert_eq!(app.language, first, "update only records the switch");

        let mut app = app_on_language_row(Language::ALL[1]);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.pending_language_switch, Some(first));
    }

    #[test]
    fn two_language_steps_before_a_flush_land_two_languages_on() {
        // Agent inputs are drained twice per loop iteration, so two steps
        // can queue up before `flush_pending_saves` performs either.
        let mut app = app_on_language_row(Language::English);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert_eq!(app.pending_language_switch, Some(Language::ALL[2]));

        let mut app = app_on_language_row(Language::English);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(
            app.pending_language_switch,
            Some(Language::ALL[Language::ALL.len() - 2])
        );
        assert_eq!(app.language, Language::English);
    }

    #[test]
    fn stepping_back_to_the_current_language_leaves_no_switch_pending() {
        let mut app = app_on_language_row(Language::Turkish);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        update(&mut app, AppEvent::Key(make_key(KeyCode::Left)));
        assert_eq!(app.pending_language_switch, None);

        // All the way round lands on the current language too.
        for _ in 0..Language::ALL.len() {
            update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        }
        assert_eq!(app.pending_language_switch, None);
        assert_eq!(app.language, Language::Turkish);
    }

    #[test]
    fn language_row_saves_nothing_itself_and_ignores_enter() {
        // The App the switch builds requests the config save (see
        // `App::switched_to`), so the row raises no flag of its own.
        let mut app = app_on_language_row(Language::English);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert!(!app.pending_config_save);
        assert!(!app.pending_stats_save);

        let mut app = app_on_language_row(Language::English);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Enter)));
        assert_eq!(app.pending_language_switch, None);
        assert!(!app.pending_config_save);
        assert_eq!(app.error_mode, ErrorMode::ForgiveMistakes);
    }

    #[test]
    fn settings_change_raises_config_save_flag() {
        let mut app = App::new();
        app.screen = AppScreen::Settings;
        app.settings_selection = 0;
        assert!(!app.pending_config_save);
        update(&mut app, AppEvent::Key(make_key(KeyCode::Right)));
        assert!(app.pending_config_save);
        assert!(!app.pending_stats_save);
    }
}
