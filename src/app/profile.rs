//! The App's saved profile: converting to and from stats and config,
//! deferred saves, and switching practice language.

use std::collections::HashMap;

use super::{App, AppScreen, ErrorMode};
use crate::config::{Config, ErrorModeSerde};
use crate::engine::filter::FocusRule;
use crate::engine::Language;
use crate::persistence::{SavedKeyStats, SavedLessonResult, SavedStats};
use crate::screens::settings::LANGUAGE_ROW;

impl App {
    /// Apply config.toml's settings to a freshly constructed App: what
    /// `main` does at launch and `switched_to` does on a language switch,
    /// kept in one place so the two cannot drift. `target_wpm`,
    /// `error_mode` and `language` are not read here, since the
    /// constructor already took them.
    pub fn apply_config(&mut self, config: &Config) {
        self.fragment_length = config.fragment_length;
        self.natural_words = config.natural_words;
        self.daily_goal_minutes = config.daily_goal_minutes;
        self.alphabet_size = config.alphabet_size;
        self.scheduler.alphabet_size = config.alphabet_size;
        // Re-run the scheduler now that the configured alphabet_size is in
        // place: the constructor ran it with the default (0.0), so letters
        // force-unlocked by alphabet_size are not yet in `active_keys`. This
        // must happen BEFORE the pin normalization below, which drops any pin
        // not in `active_keys`, otherwise a valid pin on a forced letter
        // would be dropped and erased by the next config save. It also
        // refreshes the available drill list, which the pattern pin below is
        // validated against.
        self.run_scheduler_update();
        self.set_manual_focus_from_config(config.focus_pattern.as_deref(), config.focus_letter);
        self.generator.set_natural_words(self.natural_words);
    }

    /// Perform any saves requested by `update`, clearing the flags, and
    /// any language switch it recorded. Called from main's event loop, the
    /// only place that writes to disk, logging errors to stderr without
    /// crashing.
    ///
    /// The switch goes first. It writes this language's stats to this
    /// language's file before the App is replaced, which settles any stats
    /// save already owed, and the App that replaces it requests the config
    /// save written below.
    pub fn flush_pending_saves(&mut self) {
        if let Some(language) = self.pending_language_switch.take() {
            if language != self.language {
                self.switch_language(language);
            }
        }
        if self.pending_stats_save {
            self.pending_stats_save = false;
            self.save_stats();
        }
        if self.pending_config_save {
            self.pending_config_save = false;
            if let Err(e) = self.to_config().save() {
                eprintln!("Warning: failed to save config: {e}");
            }
        }
    }

    /// Write this language's stats to its file, logging a failure.
    fn save_stats(&self) {
        if let Err(e) = self.to_saved_stats().save(self.language) {
            eprintln!("Warning: failed to save stats: {e}");
        }
    }

    /// Switch to practicing `language` the way quitting and relaunching
    /// with `--language` would: save this language's stats (quitting
    /// always does), load the new language's, and become the App
    /// `switched_to` builds from them. Disk work, so only
    /// `flush_pending_saves` calls it.
    ///
    /// A failed save is reported and does not stop the switch, just as a
    /// failed save on quit does not stop the quit.
    fn switch_language(&mut self, language: Language) {
        self.save_stats();
        let saved = SavedStats::load(language);
        *self = self.switched_to(language, saved);
    }

    /// The App a relaunch with `--language <language>` would open,
    /// restored from `saved` (that language's stats, or `None` for a fresh
    /// profile) and carrying this App's settings. Pure: `switch_language`
    /// does the loading and saving.
    ///
    /// The settings go through `to_config` and `apply_config`, the path a
    /// relaunch takes through config.toml, with one difference for pins.
    /// A relaunch cannot tell which language set a pin, so English drops
    /// one on a letter of its own that is still locked, even when another
    /// language set it, and the config save requested below would erase
    /// it at once. A switch knows every pin in this App's config came from
    /// this App, set here or held for another language, so any the new
    /// App does not apply is held instead, and switching back finds it.
    /// So a switch never erases a pin this App had; picking one in
    /// Settings still replaces it. A pin English already dropped when it
    /// read config.toml at launch is not in this App, so it is not held:
    /// that is the known limit of pin keys shared by every language, and
    /// any English config save erases it the same way.
    /// Target WPM and error mode go to the constructor from
    /// this App, as `main` passes them once the command line has had its
    /// say, so a `--target-wpm` override survives.
    ///
    /// The new App opens where the switch was made, on the Settings
    /// language row, and requests a config save so config.toml remembers
    /// the language. `running` and the menu selection carry over, so a
    /// quit queued behind the switch (agent inputs arrive in batches)
    /// still quits, and Esc returns to the menu as the user left it. So
    /// does the terminal height, which only a resize would report again.
    pub fn switched_to(&self, language: Language, saved: Option<SavedStats>) -> App {
        let mut app = App::new_with_language(self.target_wpm(), self.error_mode, saved, language);
        let config = self.to_config();
        app.apply_config(&config);
        // Hold whatever pin key the new App would leave out of the file.
        let written = app.to_config();
        if written.focus_letter.is_none() {
            app.foreign_focus_letter = config.focus_letter;
        }
        if written.focus_pattern.is_none() {
            app.foreign_focus_pattern = config.focus_pattern;
        }
        app.running = self.running;
        app.screen = AppScreen::Settings;
        app.settings_selection = LANGUAGE_ROW;
        app.menu_selection = self.menu_selection;
        app.terminal_height = self.terminal_height;
        app.pending_config_save = true;
        app
    }

    /// Convert current app state to a `SavedStats` for persistence.
    pub fn to_saved_stats(&self) -> SavedStats {
        let mut keys = HashMap::new();
        for (ch, key_stats) in &self.per_key_stats {
            keys.insert(*ch, SavedKeyStats::from(key_stats));
        }

        SavedStats {
            version: 2,
            keys,
            unlocked_letters: self.scheduler.active_keys.clone(),
            total_lessons: self.lesson_count,
            last_session: chrono_now_iso8601(),
            today_seconds_practiced: self.today_seconds_practiced,
            today_minutes_practiced: None,
            today_date: self.today_date.clone(),
            last_lesson: self.last_lesson.as_ref().map(|r| SavedLessonResult {
                wpm: r.wpm,
                accuracy: r.accuracy,
            }),
            lesson_history: self
                .lesson_history
                .iter()
                .map(|r| SavedLessonResult {
                    wpm: r.wpm,
                    accuracy: r.accuracy,
                })
                .collect(),
        }
    }

    /// Convert current app settings to a `Config` for persistence.
    pub fn to_config(&self) -> Config {
        // One pin, two keys: a letter goes to `focus_letter` exactly as
        // it always did, a pattern to `focus_pattern`. Whichever is unset
        // is omitted from the file, so a user who never opens the pattern
        // row writes byte-identical config to before.
        let (focus_letter, focus_pattern) = match self.manual_focus {
            Some(FocusRule::Key(c)) => (Some(c), None),
            Some(rule) => (None, Some(rule.config_value())),
            None => (None, None),
        };
        // A pin another language set goes back exactly as it was read
        // (see `set_manual_focus_from_config`).
        let focus_letter = focus_letter.or(self.foreign_focus_letter);
        let focus_pattern = focus_pattern.or_else(|| self.foreign_focus_pattern.clone());

        Config {
            target_wpm: self.target_wpm(),
            error_mode: match self.error_mode {
                ErrorMode::ForgiveMistakes => ErrorModeSerde::ForgiveMistakes,
                ErrorMode::StopOnError => ErrorModeSerde::StopOnError,
            },
            fragment_length: self.fragment_length,
            natural_words: self.natural_words,
            daily_goal_minutes: self.daily_goal_minutes,
            alphabet_size: self.alphabet_size,
            focus_letter,
            focus_pattern,
            language: self.language.code().to_string(),
        }
    }
}

/// Simple ISO 8601 timestamp without depending on chrono.
fn chrono_now_iso8601() -> String {
    // Use std::time to produce a Unix timestamp, format manually
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => {
            let secs = d.as_secs();
            // Approximate: good enough for a "last session" marker
            format!("{secs}")
        }
        Err(_) => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{set_active_keys, unlock_all_letters};

    #[test]
    fn to_config_carries_manual_focus() {
        let mut app = App::new();
        app.manual_focus = Some(FocusRule::Key('r'));
        assert_eq!(app.to_config().focus_letter, Some('r'));
        assert_eq!(app.to_config().focus_pattern, None);
        app.manual_focus = None;
        assert_eq!(app.to_config().focus_letter, None);
        assert_eq!(app.to_config().focus_pattern, None);
    }

    #[test]
    fn to_config_writes_a_pattern_to_its_own_key() {
        let mut app = App::new();
        app.manual_focus = Some(FocusRule::Suffix("tion"));
        let cfg = app.to_config();
        assert_eq!(cfg.focus_pattern, Some("-tion".to_string()));
        assert_eq!(
            cfg.focus_letter, None,
            "a pattern pin must not also write a letter"
        );

        app.manual_focus = Some(FocusRule::Contains("cr"));
        let cfg = app.to_config();
        assert_eq!(cfg.focus_pattern, Some("cr".to_string()));
        assert_eq!(cfg.focus_letter, None);
    }

    #[test]
    fn config_round_trips_both_pin_kinds() {
        // Letter out, letter back.
        let mut app = App::new();
        app.manual_focus = Some(FocusRule::Key('r'));
        let cfg = app.to_config();
        let mut reloaded = App::new();
        reloaded.set_manual_focus_from_config(cfg.focus_pattern.as_deref(), cfg.focus_letter);
        assert_eq!(reloaded.manual_focus, Some(FocusRule::Key('r')));

        // Pattern out, pattern back.
        unlock_all_letters(&mut app);
        app.manual_focus = Some(FocusRule::Suffix("tion"));
        let cfg = app.to_config();
        let mut reloaded = App::new();
        unlock_all_letters(&mut reloaded);
        reloaded.set_manual_focus_from_config(cfg.focus_pattern.as_deref(), cfg.focus_letter);
        assert_eq!(reloaded.manual_focus, Some(FocusRule::Suffix("tion")));
    }

    #[test]
    fn turkish_save_round_trips_through_saved_stats() {
        let mut app =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        app.per_key_stats.entry('ş').or_default().record_hit(250);
        let saved = app.to_saved_stats();
        let json = serde_json::to_string(&saved).unwrap();
        let back: SavedStats = serde_json::from_str(&json).unwrap();
        let restored = App::new_with_language(
            35,
            ErrorMode::ForgiveMistakes,
            Some(back),
            Language::Turkish,
        );
        assert!(restored.per_key_stats.contains_key(&'ş'));
        assert_eq!(restored.scheduler.active_keys, app.scheduler.active_keys);
    }

    // --- Language switch (`switched_to` is pure, so none of this touches disk) ---

    /// Saved stats for a `language` profile that has practiced `letter`,
    /// finished `lessons` lessons and unlocked its first `unlocked` letters.
    fn saved_profile(
        language: Language,
        letter: char,
        lessons: u32,
        unlocked: usize,
    ) -> SavedStats {
        let mut app = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, language);
        app.per_key_stats.entry(letter).or_default().record_hit(250);
        app.lesson_count = lessons;
        set_active_keys(&mut app, language.unlock_order()[..unlocked].to_vec());
        app.to_saved_stats()
    }

    #[test]
    fn switched_to_practices_the_new_language_from_its_own_stats() {
        let mut en = App::new();
        en.per_key_stats.entry('e').or_default().record_hit(300);
        en.lesson_count = 3;

        let de = en.switched_to(
            Language::German,
            Some(saved_profile(Language::German, 'ä', 7, 9)),
        );
        assert_eq!(de.language, Language::German);
        assert_eq!(de.lesson_count, 7);
        assert_eq!(
            de.scheduler.active_keys,
            Language::German.unlock_order()[..9].to_vec()
        );
        assert!(de.per_key_stats.contains_key(&'ä'));
        assert!(
            !de.per_key_stats.contains_key(&'e'),
            "English stats must stay with English"
        );
        for c in de.generated_text.chars() {
            assert!(c == ' ' || de.scheduler.active_keys.contains(&c), "{c}");
        }
    }

    #[test]
    fn switched_to_without_saved_stats_is_a_fresh_profile() {
        let mut en = App::new();
        en.per_key_stats.entry('e').or_default().record_hit(300);
        en.lesson_count = 3;
        unlock_all_letters(&mut en);

        let tr = en.switched_to(Language::Turkish, None);
        assert_eq!(tr.language, Language::Turkish);
        assert!(tr.per_key_stats.is_empty());
        assert_eq!(tr.lesson_count, 0);
        assert!(tr.last_lesson.is_none());
        assert_eq!(
            tr.scheduler.active_keys,
            Language::Turkish.unlock_order()[..6].to_vec()
        );
    }

    #[test]
    fn switched_to_carries_every_setting_over() {
        let mut en = App::new_with_opts(50, ErrorMode::StopOnError);
        en.fragment_length = 150;
        en.natural_words = false;
        en.daily_goal_minutes = 45;
        en.alphabet_size = 0.25;
        en.scheduler.alphabet_size = 0.25;

        let de = en.switched_to(Language::German, None);
        assert_eq!(de.target_wpm(), 50);
        assert_eq!(de.error_mode, ErrorMode::StopOnError);
        assert_eq!(de.fragment_length, 150);
        assert!(!de.natural_words);
        assert!(!de.generator.natural_words());
        assert_eq!(de.daily_goal_minutes, 45);
        assert_eq!(de.alphabet_size, 0.25);
        assert_eq!(de.scheduler.alphabet_size, 0.25);
        // Applied, not only stored: German's forced letters are unlocked
        // and the drill list follows them (the debug_assert in
        // `available_patterns` checks that).
        let forced = crate::engine::scheduler::forced_extra_letters(Language::German, 0.25);
        assert_eq!(de.scheduler.active_keys.len(), 6 + forced);
        let _ = de.available_patterns();
    }

    #[test]
    fn switched_to_keeps_a_pin_on_a_letter_both_languages_share() {
        // 'r' is a starter letter in English and Turkish alike.
        let mut en = App::new();
        en.manual_focus = Some(FocusRule::Key('r'));
        let tr = en.switched_to(Language::Turkish, None);
        assert_eq!(tr.manual_focus, Some(FocusRule::Key('r')));
        assert!(tr.focus_is_pinned());
    }

    #[test]
    fn switched_to_holds_a_pin_the_new_language_cannot_use() {
        // A Turkish pin on 'ş' (every letter forced in, so it is usable)
        // means nothing in English. English holds it for Turkish and
        // writes it back, as a relaunch through config.toml would too,
        // and switching back pins it again.
        let mut tr =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        tr.alphabet_size = 1.0;
        tr.scheduler.alphabet_size = 1.0;
        tr.run_scheduler_update();
        tr.manual_focus = Some(FocusRule::Key('ş'));
        assert!(tr.focus_is_pinned());

        let en = tr.switched_to(Language::English, None);
        assert_eq!(en.manual_focus, None, "'ş' is not an English letter");
        assert_eq!(en.to_config().focus_letter, Some('ş'));
        let back = en.switched_to(Language::Turkish, None);
        assert_eq!(back.manual_focus, Some(FocusRule::Key('ş')));

        // An English drill is held by German the same way.
        let mut en = App::new();
        unlock_all_letters(&mut en);
        en.manual_focus = Some(FocusRule::Suffix("tion"));
        let de = en.switched_to(Language::German, None);
        assert_eq!(de.manual_focus, None);
        assert_eq!(de.to_config().focus_pattern.as_deref(), Some("-tion"));
    }

    #[test]
    fn switched_to_english_and_back_keeps_a_pin_english_has_locked() {
        // 't' is a German starter letter, still locked on a fresh English
        // profile. A relaunch in English drops it as one of its own locked
        // letters; the switch knows German set it, so English holds it and
        // switching back pins it again.
        let mut de = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::German);
        de.manual_focus = Some(FocusRule::Key('t'));
        assert!(de.focus_is_pinned());
        let mut relaunched = App::new();
        relaunched.apply_config(&de.to_config());
        assert_eq!(relaunched.to_config().focus_letter, None);

        let en = de.switched_to(Language::English, None);
        assert!(!en.scheduler.active_keys.contains(&'t'));
        assert_eq!(en.manual_focus, None);
        assert_eq!(en.to_config().focus_letter, Some('t'));

        let back = en.switched_to(Language::German, None);
        assert_eq!(back.manual_focus, Some(FocusRule::Key('t')));
        assert!(back.focus_is_pinned());
    }

    #[test]
    fn switched_to_keeps_a_held_drill_through_an_english_profile_short_of_it() {
        // Drills are English only, so no other language can pin one for
        // English to drop. What can happen is an English drill German
        // holds coming back to an English profile without the letters for
        // it: the switch holds it rather than erasing it, and a profile
        // that can run it pins it again.
        let mut en = App::new();
        unlock_all_letters(&mut en);
        en.manual_focus = Some(FocusRule::Suffix("tion"));
        let de = en.switched_to(Language::German, None);

        let fresh = de.switched_to(Language::English, None);
        assert_eq!(fresh.manual_focus, None);
        assert_eq!(fresh.to_config().focus_pattern.as_deref(), Some("-tion"));

        let all = Language::English.unlock_order().len();
        let full = saved_profile(Language::English, 'e', 1, all);
        let back = fresh
            .switched_to(Language::German, None)
            .switched_to(Language::English, Some(full));
        assert_eq!(back.manual_focus, Some(FocusRule::Suffix("tion")));
        assert!(back.focus_is_pinned());
    }

    #[test]
    fn switched_to_opens_on_the_language_row_and_saves_the_language() {
        let mut en = App::new();
        en.screen = AppScreen::Settings;
        en.settings_selection = LANGUAGE_ROW;
        en.menu_selection = 2;
        en.terminal_height = 50;

        let tr = en.switched_to(Language::Turkish, None);
        assert!(tr.running);
        assert_eq!(tr.screen, AppScreen::Settings);
        assert_eq!(tr.settings_selection, LANGUAGE_ROW);
        assert_eq!(tr.menu_selection, 2);
        assert_eq!(tr.terminal_height, 50, "no resize says it again");
        assert!(
            tr.pending_config_save,
            "config.toml must remember the language"
        );
        assert_eq!(tr.to_config().language, "tr");
        assert!(!tr.pending_stats_save, "its stats were only just loaded");
        assert_eq!(tr.pending_language_switch, None);
    }

    #[test]
    fn a_quit_queued_behind_a_language_switch_still_quits() {
        let mut en = App::new();
        en.running = false;
        assert!(!en.switched_to(Language::Turkish, None).running);
    }
}
