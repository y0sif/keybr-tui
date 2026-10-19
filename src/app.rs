use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::components::settings::LANGUAGE_ROW;
use crate::config::{Config, ErrorModeSerde};
use crate::engine::filter::{FocusRule, MIN_DRILL_WORDS};
use crate::engine::{Language, LetterFilter, LetterScheduler, WordGenerator};
use crate::metrics::KeyStats;
use crate::persistence::{today_date_string, SavedKeyStats, SavedLessonResult, SavedStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorMode {
    /// Must fix errors before continuing (backspace required).
    StopOnError,
    /// Can continue past errors, backspace optional.
    ForgiveMistakes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Menu,
    Typing,
    Progress,
    Settings,
}

/// Results stored after a lesson completes.
#[derive(Debug, Clone)]
pub struct LessonResult {
    pub wpm: f64,
    pub accuracy: f64,
    /// Letter that was unlocked at the end of this lesson, if any.
    pub newly_unlocked: Option<char>,
}

/// Approximate keybr-style score from a lesson's wpm + accuracy.
/// Real keybr weights speed and accuracy together; we use a simple
/// product that rewards both — keep this in sync with any deltas
/// displayed in the dashboard.
pub fn lesson_score(wpm: f64, accuracy: f64) -> f64 {
    let acc_ratio = (accuracy / 100.0).clamp(0.0, 1.0);
    (wpm * acc_ratio * acc_ratio * 100.0).round()
}

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
fn compute_available_patterns(
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

pub struct App {
    pub running: bool,
    pub screen: AppScreen,

    // --- Text state ---
    pub generated_text: String,
    /// Index of the current target character in `generated_text`.
    pub cursor_pos: usize,
    /// Indices of characters that were typed incorrectly.
    pub error_positions: HashSet<usize>,
    /// Characters that were typed correctly on first attempt (no backspace correction).
    pub first_attempt_correct: HashSet<usize>,
    /// Characters that were corrected after an error (was wrong, then fixed).
    pub recovered_positions: HashSet<usize>,
    /// Positions that have ever been in error (tracks history, not cleared by backspace).
    pub ever_error_positions: HashSet<usize>,

    // --- Per-lesson metrics (reset each lesson) ---
    /// When the first key of the current lesson was pressed.
    pub lesson_start: Option<Instant>,
    /// Correctly typed characters this lesson (used for WPM).
    pub lesson_correct: u32,
    /// Total positions attempted this lesson (for accuracy denominator).
    pub lesson_positions: u32,
    /// Positions that had an error (first-try errors).
    pub lesson_errors: u32,

    // --- Cumulative per-key stats (persist across lessons) ---
    pub per_key_stats: HashMap<char, KeyStats>,
    /// When did the current target char become active (for reaction timing).
    pub key_target_start: Option<Instant>,

    // --- Last lesson's results (shown in stats bar during next lesson) ---
    pub last_lesson: Option<LessonResult>,
    /// Rolling in-memory history of completed lessons (capped).
    /// Not persisted — used only to compute deltas vs running average
    /// in the dashboard. Older entries fall off the front.
    pub lesson_history: Vec<LessonResult>,

    /// Number of lessons completed in the current session.
    pub lesson_count: u32,

    // --- Engine ---
    /// Practice language: alphabet, phonetic model, word list and which
    /// stats file this App reads and writes. Fixed for the life of the
    /// App: switching language in Settings replaces the whole App (see
    /// `switched_to`), so nothing derived from it can be left behind.
    pub language: Language,
    pub scheduler: LetterScheduler,
    pub generator: WordGenerator,

    // --- Settings (live-adjustable) ---
    pub error_mode: ErrorMode,
    /// Target typing speed in CPM. Internally everything uses CPM.
    /// Display as WPM = CPM / 5.
    pub target_cpm: f64,
    /// Fragment length for text generation.
    pub fragment_length: usize,
    /// When true, mix real dictionary words into generated text
    /// (falling back to the phonetic model when no word matches the
    /// active letter filter).
    pub natural_words: bool,
    /// Daily practice goal in minutes. 0 hides the daily-goal indicator.
    pub daily_goal_minutes: u32,
    /// Fraction of the non-starter alphabet to force-include regardless of
    /// confidence (keybr's `alphabetSize`, in [0.0, 1.0]). Mirrored onto
    /// `scheduler.alphabet_size` so the next scheduler update applies it.
    pub alphabet_size: f64,
    /// User-pinned focus rule (keybr's manual lesson focus, widened to
    /// cover combination drills). Overrides the scheduler's auto pick at
    /// generation time only: scheduler logic, stats recording, and unlock
    /// progression are untouched.
    ///
    /// One field holds both kinds of pin on purpose. The settings screen
    /// offers a letter row and a pattern row, and they write here, so a
    /// letter pin and a pattern pin cannot coexist by construction rather
    /// than by convention.
    pub manual_focus: Option<FocusRule>,
    /// A `focus_letter` from config.toml that this session cannot use but
    /// another language may have set, since every language shares the
    /// key. Never applied; held as read so `to_config` writes it back
    /// instead of erasing it. Set by `set_manual_focus_from_config`, and
    /// by `switched_to` for a pin the language switched from left in the
    /// file. Cleared when the user picks a pin in Settings.
    pub foreign_focus_letter: Option<char>,
    /// Same as `foreign_focus_letter`, for `focus_pattern`.
    pub foreign_focus_pattern: Option<String>,
    /// The combination drills the settings pattern row may offer right
    /// now, derived from the unlocked alphabet.
    ///
    /// Stored rather than computed on demand because deriving it scans the
    /// whole wordlist once per drill, and the settings screen and the
    /// taria tree would otherwise do that on every frame. It is also the
    /// one definition of "available", read by `effective_focus`, the
    /// settings row, and the pattern cycler alike, so the UI cannot show a
    /// drill the generator is not running.
    ///
    /// Private so `refresh_available_patterns` is the only writer, and
    /// read through `available_patterns()`. Refresh after anything that
    /// can change `scheduler.active_keys`, which in practice means after
    /// every `scheduler.update` (see `run_scheduler_update`) and after
    /// restoring saved letters at construction.
    available_patterns: Vec<FocusRule>,
    /// `scheduler.active_keys.len()` when `available_patterns` was last
    /// computed, so debug builds can catch a stale list. Not a general
    /// cache key: active keys only ever grow at runtime, so the length
    /// pins the alphabet down well enough to fail a test that forgets to
    /// refresh.
    available_patterns_for_keys: usize,

    // --- Daily-goal tracker (persisted) ---
    /// Wall-clock seconds practiced today. Display as minutes; storing in
    /// seconds avoids the floor-to-zero on sub-minute lessons.
    pub today_seconds_practiced: u32,
    /// YYYY-MM-DD this counter refers to. Reset on day rollover.
    pub today_date: String,

    // --- Navigation state ---
    /// Selected item index in the main menu.
    pub menu_selection: usize,
    /// Selected item index in the settings screen.
    pub settings_selection: usize,

    // --- Deferred persistence (MVU purity) ---
    /// Set by `update` when stats should be written; main's event loop
    /// performs the write and clears the flag. `update` itself never does
    /// disk I/O, so tests driving it can't touch the user's real files.
    pub pending_stats_save: bool,
    /// Same as `pending_stats_save`, for the config file.
    pub pending_config_save: bool,
    /// Set by `update` when the user steps the Settings language row;
    /// `flush_pending_saves` performs the switch, which saves this
    /// language's stats and loads the new one's, and clears it. `None`
    /// means no switch is owed, and so does the current language, which
    /// `update` never leaves here.
    pub pending_language_switch: Option<Language>,
}

impl App {
    pub fn new() -> Self {
        Self::new_with_opts(35, ErrorMode::ForgiveMistakes) // 35 WPM = 175 CPM
    }

    pub fn new_with_opts(target_wpm: u32, error_mode: ErrorMode) -> Self {
        Self::new_with_state(target_wpm, error_mode, None)
    }

    /// Create a new English App, optionally restoring state from saved stats.
    pub fn new_with_state(
        target_wpm: u32,
        error_mode: ErrorMode,
        saved: Option<SavedStats>,
    ) -> Self {
        Self::new_with_language(target_wpm, error_mode, saved, Language::English)
    }

    /// Create a new App practicing `language`, optionally restoring state
    /// from that language's saved stats.
    pub fn new_with_language(
        target_wpm: u32,
        error_mode: ErrorMode,
        saved: Option<SavedStats>,
        language: Language,
    ) -> Self {
        let mut scheduler = LetterScheduler::for_language(language);
        let mut stats: HashMap<char, KeyStats> = HashMap::new();
        let target_cpm = target_wpm as f64 * 5.0;
        let mut lesson_count: u32 = 0;
        let today = today_date_string();
        let mut today_seconds_practiced: u32 = 0;
        let mut today_date: String = today.clone();
        let mut last_lesson: Option<LessonResult> = None;
        let mut lesson_history: Vec<LessonResult> = Vec::new();

        // Restore from saved stats if available
        if let Some(saved) = saved {
            // Restore per-key stats
            for (ch, saved_key) in &saved.keys {
                let key_stats = stats.entry(*ch).or_default();
                key_stats.attempts = saved_key.attempts;
                key_stats.errors = saved_key.errors;
                key_stats.filtered_time_ms = saved_key.filtered_time_ms;
                key_stats.best_filtered_time_ms = saved_key.best_filtered_time_ms;
                // Restore recent times (up to 20 most recent, matching KeyStats cap)
                let recent = &saved_key.recent_times_ms;
                let start = recent.len().saturating_sub(20);
                key_stats.reaction_times_ms = recent[start..].to_vec();
            }

            // Restore unlocked letters into scheduler (letters outside the
            // language's alphabet are dropped; too few left keeps the
            // starter set, as a short list always did).
            scheduler.restore_active_keys(saved.unlocked_letters);

            lesson_count = saved.total_lessons;

            // Restore daily-goal counter (with day-rollover protection).
            // `load()` already normalises on read, but be defensive in case
            // a caller hands us a raw SavedStats from somewhere else.
            if saved.today_date == today && !today.is_empty() {
                today_seconds_practiced = saved.today_seconds_practiced;
                today_date = saved.today_date;
            } else {
                today_seconds_practiced = 0;
                today_date = today.clone();
            }

            // Restore lesson stats so the dashboard's Metrics row shows
            // the previous session's numbers (and deltas) on launch.
            // `newly_unlocked` is intentionally not persisted — the
            // unlock callout is a one-time celebration, not state.
            last_lesson = saved.last_lesson.map(|r| LessonResult {
                wpm: r.wpm,
                accuracy: r.accuracy,
                newly_unlocked: None,
            });
            lesson_history = saved
                .lesson_history
                .into_iter()
                .map(|r| LessonResult {
                    wpm: r.wpm,
                    accuracy: r.accuracy,
                    newly_unlocked: None,
                })
                .collect();
        }

        // Initial scheduler update to set focused key
        scheduler.update(&stats, target_cpm);

        let filter = LetterFilter::new(
            &scheduler.active_keys,
            scheduler.focused_key.map(FocusRule::Key),
        );
        let mut generator = WordGenerator::for_language(language);
        // Default to the natural-words blend on; `apply_config` overrides
        // this from the loaded config immediately after construction.
        generator.set_natural_words(true);
        let text = generator.generate_fragment(&filter, 100);

        // Seed the drill list for the alphabet we just restored.
        // `apply_config` re-runs the scheduler with the configured
        // `alphabet_size` and refreshes this again before anything reads it.
        let available_patterns =
            compute_available_patterns(language, &generator, &scheduler.active_keys);
        let available_patterns_for_keys = scheduler.active_keys.len();

        App {
            running: true,
            screen: AppScreen::Menu,
            generated_text: text,
            cursor_pos: 0,
            error_positions: HashSet::new(),
            first_attempt_correct: HashSet::new(),
            recovered_positions: HashSet::new(),
            ever_error_positions: HashSet::new(),
            lesson_start: None,
            lesson_correct: 0,
            lesson_positions: 0,
            lesson_errors: 0,
            per_key_stats: stats,
            key_target_start: None,
            last_lesson,
            lesson_history,
            lesson_count,
            language,
            scheduler,
            generator,
            error_mode,
            target_cpm,
            fragment_length: 100,
            natural_words: true,
            daily_goal_minutes: 30,
            alphabet_size: 0.0,
            manual_focus: None,
            foreign_focus_letter: None,
            foreign_focus_pattern: None,
            available_patterns,
            available_patterns_for_keys,
            today_seconds_practiced,
            today_date,
            menu_selection: 0,
            settings_selection: 0,
            pending_stats_save: false,
            pending_config_save: false,
            pending_language_switch: None,
        }
    }

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
    /// still quits, and Esc returns to the menu as the user left it.
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
        app.pending_config_save = true;
        app
    }

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

    /// Target WPM for display (WPM = CPM / 5).
    pub fn target_wpm(&self) -> u32 {
        (self.target_cpm / 5.0).round() as u32
    }

    /// Set the target via WPM (converts to CPM internally).
    pub fn set_target_wpm(&mut self, wpm: u32) {
        self.target_cpm = wpm as f64 * 5.0;
    }

    /// WPM for the current lesson so far.
    /// Only first-attempt correct characters count toward speed.
    pub fn lesson_wpm(&self) -> f64 {
        let start = match self.lesson_start {
            Some(s) => s,
            None => return 0.0,
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        if elapsed_secs < 1.0 {
            return 0.0;
        }
        // Use first_attempt_correct count for accurate WPM
        let correct_chars = self.first_attempt_correct.len() as f64;
        (correct_chars / 5.0) / (elapsed_secs / 60.0)
    }

    /// Mean WPM of all lessons in `lesson_history` *excluding* the most
    /// recent one, so deltas computed from this don't self-compare.
    /// Returns `None` when there's no prior lesson to compare against.
    pub fn prev_mean_wpm(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1].iter().map(|r| r.wpm).sum();
        Some(sum / (n - 1) as f64)
    }

    /// Mean accuracy of all lessons *excluding* the most recent one.
    pub fn prev_mean_accuracy(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1]
            .iter()
            .map(|r| r.accuracy)
            .sum();
        Some(sum / (n - 1) as f64)
    }

    /// Mean score (derived from wpm + accuracy) of all lessons except the last.
    pub fn prev_mean_score(&self) -> Option<f64> {
        let n = self.lesson_history.len();
        if n < 2 {
            return None;
        }
        let sum: f64 = self.lesson_history[..n - 1]
            .iter()
            .map(|r| lesson_score(r.wpm, r.accuracy))
            .sum();
        Some(sum / (n - 1) as f64)
    }

    /// Accuracy for the current lesson so far.
    pub fn lesson_accuracy(&self) -> f64 {
        if self.lesson_positions == 0 {
            return 100.0;
        }
        ((self.lesson_positions - self.lesson_errors) as f64 / self.lesson_positions as f64) * 100.0
    }

    /// Called when the user finishes typing all chars in the current batch.
    /// Saves lesson results, runs scheduler, transitions to summary screen.
    pub fn finish_lesson(&mut self) {
        self.lesson_count += 1;
        let wpm = self.lesson_wpm();
        let accuracy = self.lesson_accuracy();

        let old_count = self.scheduler.active_keys.len();

        // Fold each key's lesson-mean latency into its smoothed time before
        // the scheduler reads confidences — keybr updates stats per result.
        for stats in self.per_key_stats.values_mut() {
            stats.finish_lesson();
        }

        // Update scheduler with current stats (and the drill list, which
        // a newly unlocked letter can widen).
        self.run_scheduler_update();

        let newly_unlocked = if self.scheduler.active_keys.len() > old_count {
            Some(*self.scheduler.active_keys.last().unwrap())
        } else {
            None
        };

        let result = LessonResult {
            wpm,
            accuracy,
            newly_unlocked,
        };
        // Push to in-memory history so the dashboard can compute deltas
        // against the running mean. Cap so a long session can't grow this
        // unboundedly.
        const HISTORY_CAP: usize = 50;
        self.lesson_history.push(result.clone());
        if self.lesson_history.len() > HISTORY_CAP {
            let overflow = self.lesson_history.len() - HISTORY_CAP;
            self.lesson_history.drain(0..overflow);
        }
        self.last_lesson = Some(result);

        // Immediately roll into the next lesson — no separate summary screen.
        // `start_next_lesson` regenerates text, resets per-lesson counters,
        // and sets `screen = AppScreen::Typing`.
        self.start_next_lesson();
    }

    /// Called when the user dismisses the lesson summary (any key).
    /// Generates new text and returns to the typing screen.
    pub fn start_next_lesson(&mut self) {
        // Propagate the current natural-words preference into the
        // generator before regenerating, so config changes take effect
        // at the next lesson boundary.
        self.generator.set_natural_words(self.natural_words);
        let filter = LetterFilter::new(&self.scheduler.active_keys, self.effective_focus());
        self.generated_text = self
            .generator
            .generate_fragment(&filter, self.fragment_length);
        self.cursor_pos = 0;
        self.error_positions.clear();
        self.first_attempt_correct.clear();
        self.recovered_positions.clear();
        self.ever_error_positions.clear();
        self.key_target_start = None;
        self.lesson_start = None;
        self.lesson_correct = 0;
        self.lesson_positions = 0;
        self.lesson_errors = 0;
        self.screen = AppScreen::Typing;
    }

    /// Convert current app state to a `SavedStats` for persistence.
    pub fn to_saved_stats(&self) -> SavedStats {
        let mut keys = HashMap::new();
        for (ch, key_stats) in &self.per_key_stats {
            // Keep up to 50 recent reaction times for persistence
            let recent: Vec<u64> = key_stats.reaction_times_ms.to_vec();
            keys.insert(
                *ch,
                SavedKeyStats {
                    attempts: key_stats.attempts,
                    errors: key_stats.errors,
                    filtered_time_ms: key_stats.filtered_time_ms,
                    best_filtered_time_ms: key_stats.best_filtered_time_ms,
                    recent_times_ms: recent,
                },
            );
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

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::filter::FOCUS_PATTERNS;
    use crate::engine::scheduler::UNLOCK_ORDER;

    #[test]
    fn accuracy_is_100_with_no_errors() {
        let mut app = App::new();
        app.lesson_positions = 20;
        app.lesson_errors = 0;
        assert!((app.lesson_accuracy() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn accuracy_reflects_errors() {
        let mut app = App::new();
        app.lesson_positions = 10;
        app.lesson_errors = 2;
        // (10 - 2) / 10 * 100 = 80.0
        assert!((app.lesson_accuracy() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wpm_is_zero_before_lesson_starts() {
        let app = App::new();
        assert!((app.lesson_wpm() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn target_wpm_conversion() {
        let app = App::new_with_opts(35, ErrorMode::ForgiveMistakes);
        assert_eq!(app.target_wpm(), 35);
        assert!((app.target_cpm - 175.0).abs() < f64::EPSILON);
    }

    #[test]
    fn new_with_opts_sets_values() {
        let app = App::new_with_opts(50, ErrorMode::StopOnError);
        assert_eq!(app.target_wpm(), 50);
        assert_eq!(app.error_mode, ErrorMode::StopOnError);
    }

    #[test]
    fn default_target_is_35_wpm() {
        let app = App::new();
        assert_eq!(app.target_wpm(), 35);
        assert!((app.target_cpm - 175.0).abs() < f64::EPSILON);
    }

    /// Unlock the whole alphabet, which is what makes every combination
    /// drill reachable. Mirrors what the scheduler does over a long
    /// profile, without having to type thousands of characters.
    ///
    /// Writing `active_keys` by hand bypasses `run_scheduler_update`, so
    /// the derived drill list has to be refreshed the same way the
    /// scheduler path does it.
    fn unlock_all_letters(app: &mut App) {
        set_active_keys(app, ('a'..='z').collect());
    }

    /// Replace the unlocked alphabet and refresh what is derived from it.
    /// The only way these tests should write `active_keys`.
    fn set_active_keys(app: &mut App, keys: Vec<char>) {
        app.scheduler.active_keys = keys;
        app.refresh_available_patterns();
    }

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
    fn finishing_a_lesson_refreshes_the_drill_list() {
        // `finish_lesson` runs the scheduler, which can unlock a letter.
        // The debug assertion inside `available_patterns` catches a list
        // left behind; this pins the refresh down in release builds too.
        let mut app = App::new();
        app.alphabet_size = 1.0;
        app.scheduler.alphabet_size = 1.0;
        app.generated_text = "en".to_string();
        app.finish_lesson();
        assert_eq!(app.available_patterns().len(), FOCUS_PATTERNS.len());
    }

    #[test]
    fn pinned_letter_appears_in_every_generated_word() {
        let mut app = App::new();
        app.manual_focus = Some(FocusRule::Key('r'));
        app.start_next_lesson();
        assert!(!app.generated_text.is_empty());
        for word in app.generated_text.split_whitespace() {
            assert!(
                word.contains('r'),
                "pinned 'r' missing from word '{}' in: {}",
                word,
                app.generated_text
            );
        }
    }

    #[test]
    fn pinned_pattern_is_satisfied_by_every_generated_word() {
        for rule in [FocusRule::Contains("cr"), FocusRule::Suffix("ing")] {
            let mut app = App::new();
            unlock_all_letters(&mut app);
            assert!(app.available_patterns().contains(&rule));
            app.manual_focus = Some(rule);
            app.start_next_lesson();
            assert!(!app.generated_text.is_empty());
            for word in app.generated_text.split_whitespace() {
                assert!(
                    rule.matches(word),
                    "pinned {} not satisfied by word '{}' in: {}",
                    rule.config_value(),
                    word,
                    app.generated_text
                );
            }
        }
    }

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
    fn turkish_app_starts_with_turkish_letters_and_no_drills() {
        let mut app =
            App::new_with_language(35, ErrorMode::ForgiveMistakes, None, Language::Turkish);
        assert_eq!(
            app.scheduler.active_keys,
            vec!['a', 'e', 'i', 'n', 'ı', 'r']
        );
        assert!(app.available_patterns().is_empty());
        for c in app.generated_text.chars() {
            assert!(c == ' ' || app.scheduler.active_keys.contains(&c), "{c}");
        }
        // Unlock everything: still no English drills on offer.
        set_active_keys(&mut app, Language::Turkish.unlock_order().to_vec());
        assert!(app.available_patterns().is_empty());
        app.start_next_lesson();
        assert!(app.generated_text.chars().count() >= app.fragment_length);
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
        let mut app = App::new();
        for (pattern, letter) in [
            (None, Some('z')),
            (Some("-tion"), None),
            (Some("banana"), None),
            (None, Some('1')),
            (None, Some('é')),
        ] {
            app.set_manual_focus_from_config(pattern, letter);
            assert_eq!(app.manual_focus, None, "{pattern:?} {letter:?}");
            let cfg = app.to_config();
            assert_eq!(cfg.focus_letter, None, "{pattern:?} {letter:?}");
            assert_eq!(cfg.focus_pattern, None, "{pattern:?} {letter:?}");
        }
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

        let tr = en.switched_to(Language::Turkish, None);
        assert!(tr.running);
        assert_eq!(tr.screen, AppScreen::Settings);
        assert_eq!(tr.settings_selection, LANGUAGE_ROW);
        assert_eq!(tr.menu_selection, 2);
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
