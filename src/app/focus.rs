//! Focus pins and the combination drills the unlocked alphabet can offer:
//! the effective focus, the stored drill list, and resolving config pins.

use std::collections::HashSet;

use super::App;
use crate::engine::filter::{FocusRule, MIN_DRILL_WORDS};
use crate::engine::{Language, WordGenerator};

/// The language's combination drills a given unlocked alphabet can actually
/// drill: every letter of the pattern unlocked, and at least
/// `MIN_DRILL_WORDS` distinct real words behind it.
///
/// Both gates are needed. The letters test is cheap and rejects most
/// patterns early in the curriculum; the word count then rules out the
/// ones that are spellable in principle but have no pool behind them, so
/// a selectable drill is never a single word repeated for a whole lesson.
///
/// A free function rather than a method because `App::new_with_state`
/// needs it before there is an `App` to call it on.
pub(super) fn compute_available_patterns(
    language: Language,
    generator: &WordGenerator,
    active_keys: &[char],
) -> Vec<FocusRule> {
    let allowed: HashSet<char> = active_keys.iter().copied().collect();
    let dict = generator.dictionary();
    language
        .focus_patterns()
        .iter()
        .copied()
        .filter(|rule| rule.letters().iter().all(|c| allowed.contains(c)))
        .filter(|rule| dict.has_enough_matches(rule, &allowed, MIN_DRILL_WORDS))
        .collect()
}

impl App {
    /// The rule the generator should force onto every word: the manual
    /// pin when set and still reachable, otherwise the scheduler's auto
    /// pick. A stale pin (e.g. a hand-edited config naming a locked
    /// letter, or a pattern whose letters were force-unlocked by an
    /// `alphabet_size` the user has since lowered) falls back to auto
    /// silently instead of forcing unpracticed keys.
    ///
    /// The two pin kinds are guarded differently because they mean
    /// different things. A letter pin needs only its key unlocked, which
    /// is the cheap membership test it has always been. A pattern pin must
    /// be in `available_patterns`, the exact list the settings row offered
    /// it from: anything weaker would let the UI report an active drill
    /// while the generator quietly emitted unfocused words.
    ///
    /// This runs on every render, and both guards are a slice scan over a
    /// handful of entries. Nothing here touches the dictionary.
    pub fn effective_focus(&self) -> Option<FocusRule> {
        self.manual_focus
            .filter(|rule| match rule {
                FocusRule::Key(_) => self.focus_letters_unlocked(rule),
                FocusRule::Contains(_) | FocusRule::Suffix(_) => {
                    self.available_patterns().contains(rule)
                }
            })
            .or(self.scheduler.focused_key.map(FocusRule::Key))
    }

    /// True when every letter a rule needs is currently unlocked.
    fn focus_letters_unlocked(&self, rule: &FocusRule) -> bool {
        rule.letters()
            .iter()
            .all(|c| self.scheduler.active_keys.contains(c))
    }

    /// The combination drills the settings screen may offer right now.
    ///
    /// Read-only view of the stored list. Views call this; nothing here
    /// computes, so a render never scans the wordlist.
    pub fn available_patterns(&self) -> &[FocusRule] {
        debug_assert_eq!(
            self.available_patterns_for_keys,
            self.scheduler.active_keys.len(),
            "available_patterns is stale: refresh_available_patterns must follow \
             every change to scheduler.active_keys",
        );
        &self.available_patterns
    }

    /// Recompute the stored drill list for the current unlocked alphabet.
    ///
    /// Call after anything that can change `scheduler.active_keys`.
    pub fn refresh_available_patterns(&mut self) {
        self.available_patterns =
            compute_available_patterns(self.language, &self.generator, &self.scheduler.active_keys);
        self.available_patterns_for_keys = self.scheduler.active_keys.len();
    }

    /// Run the letter scheduler and refresh whatever is derived from the
    /// alphabet it just changed.
    ///
    /// The only way production code steps the scheduler on an `App`, so
    /// the drill list cannot be left behind by a new caller.
    pub fn run_scheduler_update(&mut self) {
        self.scheduler.update(&self.per_key_stats, self.target_cpm);
        self.refresh_available_patterns();
    }

    /// True when `effective_focus()` is the user's manual pin rather than
    /// the scheduler's auto pick. A stale pin (letter no longer unlocked)
    /// already fell back to auto inside `effective_focus`, so it reports
    /// false here. Shared by every component that styles the pin
    /// differently from the auto focus, so they can't drift apart.
    pub fn focus_is_pinned(&self) -> bool {
        self.manual_focus.is_some() && self.effective_focus() == self.manual_focus
    }

    /// Resolve the two config pin keys into the single `manual_focus`
    /// field, applying the documented precedence: a usable
    /// `focus_pattern` wins, else `focus_letter`, else Auto.
    ///
    /// Both are validated and silently dropped when unusable, rather than
    /// reported as an error. The Settings rows read `manual_focus`
    /// directly, so keeping an unusable pin here would display "pinned"
    /// while lessons behave as Auto, and the next config save would
    /// persist that lie. A pattern must still be in `available_patterns`
    /// (letters unlocked *and* words behind it); a letter is lowercased
    /// first (with the language's case rules, so Turkish `"I"` is `ı`),
    /// because the config is a plain TOML file and a hand-edited
    /// `"R"` should pin 'r' rather than silently misbehave, then dropped
    /// unless the letter is unlocked. `effective_focus` keeps its own
    /// guard as defense in depth for pins set at runtime.
    ///
    /// The exception is an unusable pin another language may have set
    /// (see `is_foreign_pin`). It is not applied either, but is kept in
    /// `foreign_focus_letter` / `foreign_focus_pattern` exactly as read,
    /// so a session in this language does not erase it from the file.
    ///
    /// Callers must run `run_scheduler_update` with the final
    /// `alphabet_size` applied *before* calling this, so letters
    /// force-unlocked by that setting count as valid pins and the stored
    /// drill list this validates against is current.
    pub fn set_manual_focus_from_config(&mut self, pattern: Option<&str>, letter: Option<char>) {
        let stored_pattern = pattern.and_then(FocusRule::from_config);
        let from_pattern = stored_pattern.filter(|rule| self.available_patterns().contains(rule));
        let stored_letter = letter.map(|c| self.language.lower(c));
        let from_letter = stored_letter
            .map(FocusRule::Key)
            .filter(|rule| self.focus_letters_unlocked(rule));
        self.manual_focus = from_pattern.or(from_letter);

        let foreign_pattern = from_pattern.is_none()
            && stored_pattern
                .is_some_and(|rule| self.is_foreign_pin(|l| l.focus_patterns().contains(&rule)));
        self.foreign_focus_pattern = pattern.filter(|_| foreign_pattern).map(str::to_string);
        let foreign_letter = from_letter.is_none()
            && stored_letter.is_some_and(|c| self.is_foreign_pin(|l| l.is_letter(c)));
        self.foreign_focus_letter = letter.filter(|_| foreign_letter);
    }

    /// True when a config pin this session cannot use may have been set
    /// in another language, so it must stay in config.toml. `owns` says
    /// whether a language could have set it: the letter is in its
    /// alphabet, or the drill is one it offers.
    ///
    /// The pin keys record no language, so this goes by who could own the
    /// pin. English keeps its rule from before languages existed: a letter
    /// or drill of its own that it cannot use yet (locked, or a drill short
    /// of words) is dropped, and only a pin it could never have set, like
    /// Turkish `ş`, is kept. Any other language keeps every pin another
    /// language could own, since an English pin on a letter they share,
    /// still locked here, looks exactly like one of its own.
    fn is_foreign_pin(&self, owns: impl Fn(Language) -> bool) -> bool {
        let others_own = Language::ALL
            .iter()
            .any(|&other| other != self.language && owns(other));
        others_own && (self.language != Language::English || !owns(self.language))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{set_active_keys, unlock_all_letters};
    use crate::app::ErrorMode;
    use crate::config::Config;
    use crate::engine::filter::FOCUS_PATTERNS;
    use crate::engine::scheduler::UNLOCK_ORDER;

    /// How many dictionary words satisfy `rule` under `allowed`.
    ///
    /// `has_enough_matches` answers a threshold, not a count, so raise
    /// the threshold until it says no. Tests use this to state a pool
    /// size as a number, which a reader can check against the wordlist,
    /// instead of re-deriving the gate's own predicate.
    fn count_drill_words(app: &App, rule: &FocusRule, allowed: &HashSet<char>) -> usize {
        let dict = app.generator.dictionary();
        let mut found = 0;
        while dict.has_enough_matches(rule, allowed, found + 1) {
            found += 1;
        }
        found
    }

    #[test]
    fn effective_focus_prefers_unlocked_pin() {
        let mut app = App::new();
        // 'r' is one of the six starter letters, unlocked from lesson one.
        app.manual_focus = Some(FocusRule::Key('r'));
        assert_eq!(app.effective_focus(), Some(FocusRule::Key('r')));
    }

    #[test]
    fn effective_focus_falls_back_when_pin_is_locked() {
        let mut app = App::new();
        // 'z' is locked on a fresh profile; the stale pin must yield to
        // the scheduler's auto pick.
        assert!(!app.scheduler.active_keys.contains(&'z'));
        app.manual_focus = Some(FocusRule::Key('z'));
        assert!(app.scheduler.focused_key.is_some());
        assert_eq!(
            app.effective_focus(),
            app.scheduler.focused_key.map(FocusRule::Key)
        );
    }

    #[test]
    fn effective_focus_is_auto_pick_when_unpinned() {
        let app = App::new();
        assert_eq!(app.manual_focus, None);
        assert_eq!(
            app.effective_focus(),
            app.scheduler.focused_key.map(FocusRule::Key)
        );
    }

    #[test]
    fn effective_focus_keeps_a_pattern_whose_letters_are_all_unlocked() {
        let mut app = App::new();
        unlock_all_letters(&mut app);
        app.manual_focus = Some(FocusRule::Contains("cr"));
        assert_eq!(app.effective_focus(), Some(FocusRule::Contains("cr")));
        assert!(app.focus_is_pinned());
    }

    #[test]
    fn effective_focus_drops_a_pattern_with_a_locked_letter() {
        let mut app = App::new();
        // Starter alphabet: 'c' is locked, so "cr" cannot be drilled and
        // must fall back to the scheduler's auto pick rather than force
        // an unpracticed key into every word.
        assert!(!app.scheduler.active_keys.contains(&'c'));
        app.manual_focus = Some(FocusRule::Contains("cr"));
        assert_eq!(
            app.effective_focus(),
            app.scheduler.focused_key.map(FocusRule::Key)
        );
        assert!(!app.focus_is_pinned());

        // Every letter of "-tion" unlocked except one is still a drop.
        unlock_all_letters(&mut app);
        set_active_keys(&mut app, ('a'..='z').filter(|&c| c != 'o').collect());
        app.manual_focus = Some(FocusRule::Suffix("tion"));
        assert!(!app.focus_is_pinned());
    }

    #[test]
    fn available_patterns_is_empty_on_a_fresh_profile() {
        // The six starters are e,n,i,a,r,l. The only drill they spell at
        // all is "-er", and the wordlist has three of those, so the row
        // opens with nothing. Combination drills are the second half of
        // the feature: they belong to a profile past the basics, not to
        // one that has never finished a lesson.
        let app = App::new();
        assert!(
            app.available_patterns().is_empty(),
            "a fresh profile must offer no drill, got {:?}",
            app.available_patterns()
        );
    }

    #[test]
    fn the_first_drill_appears_when_t_unlocks() {
        // Seventh letter in the unlock order is 't', which takes "-er"
        // from three words to twelve and makes it the first real drill.
        let mut app = App::new();
        set_active_keys(&mut app, UNLOCK_ORDER[..7].to_vec());
        assert_eq!(
            app.available_patterns(),
            [FocusRule::Suffix("er")],
            "the first offer should be -ER alone",
        );
    }

    #[test]
    fn available_patterns_never_offers_a_drill_below_the_word_threshold() {
        // Four concrete points, two crossings of MIN_DRILL_WORDS, with
        // the words counted rather than the gate's own predicate
        // re-derived. Every letter of both rules is unlocked at every
        // point below, so the word count is the only thing deciding.
        //
        // "-er" goes from three words at six letters to twelve when 't'
        // unlocks. "-tion" goes from five at eight letters to exactly
        // eight at nine, which pins the boundary itself: a pool of
        // exactly MIN_DRILL_WORDS is offered rather than withheld.
        let mut app = App::new();
        for (n, rule, words, offered) in [
            (6, FocusRule::Suffix("er"), 3, false),
            (7, FocusRule::Suffix("er"), 12, true),
            (8, FocusRule::Suffix("tion"), 5, false),
            (9, FocusRule::Suffix("tion"), 8, true),
        ] {
            set_active_keys(&mut app, UNLOCK_ORDER[..n].to_vec());
            let allowed: HashSet<char> = UNLOCK_ORDER[..n].iter().copied().collect();
            assert!(
                rule.letters().iter().all(|c| allowed.contains(c)),
                "{} is unspellable at {n} letters, which is not the case under test",
                rule.config_value(),
            );
            assert_eq!(
                count_drill_words(&app, &rule, &allowed),
                words,
                "the wordlist moved under {} at {n} letters",
                rule.config_value(),
            );
            assert_eq!(
                app.available_patterns().contains(&rule),
                offered,
                "{} has {words} words at {n} letters, threshold is {MIN_DRILL_WORDS}",
                rule.config_value(),
            );
        }
    }

    #[test]
    fn a_single_word_drill_is_not_offered() {
        // Ten letters spell exactly one "-ous" word, "serious". A lesson
        // pinned to it would print that word forty times, so the row must
        // not offer it, even though a one-match gate would.
        let mut app = App::new();
        set_active_keys(&mut app, UNLOCK_ORDER[..10].to_vec());
        let allowed: HashSet<char> = UNLOCK_ORDER[..10].iter().copied().collect();
        let rule = FocusRule::Suffix("ous");
        let dict = app.generator.dictionary();
        assert!(
            dict.has_enough_matches(&rule, &allowed, 1),
            "the one word is there",
        );
        assert!(
            !app.available_patterns().contains(&rule),
            "one word is not a drill",
        );
    }

    #[test]
    fn available_patterns_is_empty_when_nothing_is_spellable() {
        // Four letters reach no drill at all. This is the state the
        // settings row reports as "None yet" rather than an empty box.
        let mut app = App::new();
        set_active_keys(&mut app, vec!['e', 'n', 'i', 'a']);
        assert!(app.available_patterns().is_empty());
    }

    #[test]
    fn available_patterns_grows_as_letters_unlock() {
        let mut app = App::new();
        let fresh = app.available_patterns().len();
        unlock_all_letters(&mut app);
        let full = app.available_patterns();
        assert!(
            full.len() > fresh,
            "unlocking the alphabet must open up drills"
        );
        // With every letter available the whole preset list is reachable.
        assert_eq!(full.len(), FOCUS_PATTERNS.len());
    }

    #[test]
    fn available_patterns_never_offers_a_drill_with_no_words() {
        let mut app = App::new();
        unlock_all_letters(&mut app);
        // 'g' gone: "-ing" and "gr" are unspellable, so neither may be
        // offered. Losing one letter must withdraw exactly the drills
        // that spell it and leave the other fourteen alone, so this is
        // an exact set rather than a scan that re-derives the gate.
        set_active_keys(&mut app, ('a'..='z').filter(|&c| c != 'g').collect());
        let patterns = app.available_patterns().to_vec();
        let expected: Vec<FocusRule> = FOCUS_PATTERNS
            .iter()
            .copied()
            .filter(|r| !r.letters().contains(&'g'))
            .collect();
        assert_eq!(patterns, expected);
        assert_eq!(patterns.len(), FOCUS_PATTERNS.len() - 2);
    }

    #[test]
    fn effective_focus_drops_a_pattern_that_is_no_longer_on_offer() {
        // The divergence this guard exists for: every letter of "-ous" is
        // unlocked at ten letters, but the wordlist holds exactly one such
        // word there, so the drill is not on offer. The letters test alone
        // would keep the pin and let the UI announce a drill the generator
        // is not running.
        let mut app = App::new();
        unlock_all_letters(&mut app);
        let rule = FocusRule::Suffix("ous");
        app.manual_focus = Some(rule);
        assert_eq!(app.effective_focus(), Some(rule));
        assert!(app.focus_is_pinned());

        set_active_keys(&mut app, UNLOCK_ORDER[..10].to_vec());
        assert!(
            rule.letters()
                .iter()
                .all(|c| app.scheduler.active_keys.contains(c)),
            "the letters are all unlocked, which is the point",
        );
        assert!(!app.available_patterns().contains(&rule));
        assert_eq!(
            app.effective_focus(),
            app.scheduler.focused_key.map(FocusRule::Key),
            "an unavailable drill must fall back to the auto pick",
        );
        assert!(!app.focus_is_pinned());
    }

    #[test]
    fn effective_focus_keeps_a_letter_pin_on_the_letters_test_alone() {
        // The letter path must not pick up the drill gate: 'r' is
        // unlocked from lesson one and stays pinned on a fresh profile
        // that offers no drill at all.
        let mut app = App::new();
        assert!(app.available_patterns().is_empty());
        app.manual_focus = Some(FocusRule::Key('r'));
        assert_eq!(app.effective_focus(), Some(FocusRule::Key('r')));
        assert!(app.focus_is_pinned());
    }

    #[test]
    fn the_drill_list_is_refreshed_when_the_scheduler_runs() {
        // `alphabet_size` does not move `active_keys` by itself; the
        // letters it forces are added by the next scheduler run, and the
        // stored drill list must follow them there.
        let mut app = App::new();
        assert!(app.available_patterns().is_empty());

        app.alphabet_size = 1.0;
        app.scheduler.alphabet_size = 1.0;
        assert!(
            app.available_patterns().is_empty(),
            "the setting alone unlocks nothing",
        );

        app.run_scheduler_update();
        assert_eq!(app.scheduler.active_keys.len(), UNLOCK_ORDER.len());
        assert_eq!(
            app.available_patterns().len(),
            FOCUS_PATTERNS.len(),
            "the whole alphabet must open every drill",
        );
    }

    #[test]
    fn config_pattern_outranks_config_letter() {
        // A file carrying both (hand-edited, or written by a build that
        // stored them separately) resolves to exactly one pin.
        let mut app = App::new();
        unlock_all_letters(&mut app);
        app.set_manual_focus_from_config(Some("cr"), Some('r'));
        assert_eq!(app.manual_focus, Some(FocusRule::Contains("cr")));
    }

    #[test]
    fn config_falls_back_to_the_letter_when_the_pattern_is_unusable() {
        let mut app = App::new();
        // Starter alphabet: "cr" is not available, 'r' is unlocked.
        app.set_manual_focus_from_config(Some("cr"), Some('r'));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('r')));

        // Junk in the pattern key is a dropped pin, not an error.
        app.set_manual_focus_from_config(Some("banana"), Some('r'));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('r')));

        // Neither usable: Auto.
        app.set_manual_focus_from_config(Some("banana"), Some('z'));
        assert_eq!(app.manual_focus, None);
        app.set_manual_focus_from_config(None, None);
        assert_eq!(app.manual_focus, None);
    }

    #[test]
    fn config_pin_normalizes_uppercase_to_valid_pin() {
        let mut app = App::new();
        // A hand-edited config may hold "R"; 'r' is a starter letter, so
        // the lowered pin is valid and must survive.
        app.set_manual_focus_from_config(None, Some('R'));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('r')));
    }

    #[test]
    fn config_pin_on_locked_letter_is_dropped() {
        let mut app = App::new();
        // 'z' is locked on a fresh profile — the pin must be dropped so
        // the Settings label doesn't claim a pin that behaves as Auto.
        assert!(!app.scheduler.active_keys.contains(&'z'));
        app.set_manual_focus_from_config(None, Some('z'));
        assert_eq!(app.manual_focus, None);
    }

    #[test]
    fn config_pin_on_forced_letter_survives_scheduler_rerun() {
        let mut app = App::new();
        // The real load order, `apply_config`: mirror the configured
        // alphabet_size onto the scheduler, call `run_scheduler_update`
        // so the force-unlocked letters join `active_keys`, THEN validate
        // the pin. alphabet_size 0.05 forces one extra letter: 't'.
        //
        // `run_scheduler_update` rather than a bare `scheduler.update` is
        // what keeps the derived drill list in step with the alphabet the
        // scheduler just widened.
        app.apply_config(&Config {
            alphabet_size: 0.05,
            focus_letter: Some('t'),
            ..Config::default()
        });
        assert!(app.scheduler.active_keys.contains(&'t'));
        // Reading the derived list here is the staleness check: with a
        // bare `scheduler.update` the cache is a letter behind and this
        // trips the debug_assert in `available_patterns`.
        let _ = app.available_patterns();
        assert_eq!(app.manual_focus, Some(FocusRule::Key('t')));
    }

    #[test]
    fn focus_is_pinned_tracks_pin_validity() {
        let mut app = App::new();
        // No pin: auto focus is never "pinned".
        assert!(!app.focus_is_pinned());
        // Valid pin on an unlocked starter letter.
        app.manual_focus = Some(FocusRule::Key('r'));
        assert!(app.focus_is_pinned());
        // Stale pin on a locked letter falls back to auto and must not
        // report as pinned.
        app.manual_focus = Some(FocusRule::Key('z'));
        assert!(!app.focus_is_pinned());
    }

    #[test]
    fn turkish_config_pin_uses_turkish_lowercase() {
        let mut app =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        // Hand-edited uppercase dotless I pins 'ı', which is a starter.
        app.set_manual_focus_from_config(None, Some('I'));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('ı')));
        app.set_manual_focus_from_config(None, Some('İ'));
        assert_eq!(app.manual_focus, Some(FocusRule::Key('i')));
        assert_eq!(app.to_config().language, "tr");
    }

    #[test]
    fn english_pins_survive_a_turkish_session_config_save() {
        // Every language shares the pin keys. Turkish cannot use an
        // English pin on 'l' (locked here), on 'q' (not a Turkish letter)
        // or on an English drill, so it applies none of them, but its
        // next config save must leave each one in the file.
        let mut tr =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        assert!(!tr.scheduler.active_keys.contains(&'l'));
        for (pattern, letter) in [(None, Some('l')), (None, Some('q')), (Some("-tion"), None)] {
            tr.set_manual_focus_from_config(pattern, letter);
            assert_eq!(tr.manual_focus, None, "{pattern:?} {letter:?}");
            let cfg = tr.to_config();
            assert_eq!(cfg.focus_letter, letter);
            assert_eq!(cfg.focus_pattern.as_deref(), pattern);
            assert_eq!(cfg.language, "tr");
        }

        // Back in English, 'l' is a starter letter and pins again.
        tr.set_manual_focus_from_config(None, Some('l'));
        let cfg = tr.to_config();
        let mut en = App::new();
        en.set_manual_focus_from_config(cfg.focus_pattern.as_deref(), cfg.focus_letter);
        assert_eq!(en.manual_focus, Some(FocusRule::Key('l')));
    }

    #[test]
    fn a_turkish_letter_pin_survives_an_english_session_config_save() {
        let mut app = App::new();
        app.set_manual_focus_from_config(None, Some('ş'));
        assert_eq!(app.manual_focus, None, "'ş' is not an English letter");
        assert!(!app.focus_is_pinned());
        assert_eq!(app.to_config().focus_letter, Some('ş'));
    }

    #[test]
    fn foreign_letter_pins_survive_german_and_english_sessions() {
        // German cannot use an English pin on 'q' (a German letter, locked
        // here) or a Turkish pin on 'ş' (not a German letter), but keeps both.
        let mut de = App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::German);
        assert!(Language::German.is_letter('q'));
        assert!(!de.scheduler.active_keys.contains(&'q'));
        for letter in ['q', 'ş'] {
            de.set_manual_focus_from_config(None, Some(letter));
            assert_eq!(de.manual_focus, None, "{letter}");
            let cfg = de.to_config();
            assert_eq!(cfg.focus_letter, Some(letter));
            assert_eq!(cfg.language, "de");
        }

        // English keeps a German pin on 'ß', which it could never have set.
        let mut en = App::new();
        en.set_manual_focus_from_config(None, Some('ß'));
        assert_eq!(en.manual_focus, None, "'ß' is not an English letter");
        assert_eq!(en.to_config().focus_letter, Some('ß'));
    }

    #[test]
    fn english_still_erases_pins_it_cannot_use() {
        // The English-only path is unchanged: a pin on a locked English
        // letter, an English drill short of letters, junk, and a letter no
        // supported language has are dropped, and the next save erases
        // them, exactly as before languages existed.
        assert!(
            Language::ALL.iter().all(|l| !l.is_letter('þ')),
            "the test needs a letter no supported language has"
        );
        let mut app = App::new();
        for (pattern, letter) in [
            (None, Some('z')),
            (Some("-tion"), None),
            (Some("banana"), None),
            (None, Some('1')),
            (None, Some('þ')),
        ] {
            app.set_manual_focus_from_config(pattern, letter);
            assert_eq!(app.manual_focus, None, "{pattern:?} {letter:?}");
            let cfg = app.to_config();
            assert_eq!(cfg.focus_letter, None, "{pattern:?} {letter:?}");
            assert_eq!(cfg.focus_pattern, None, "{pattern:?} {letter:?}");
        }
    }
}
