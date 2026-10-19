use std::collections::HashMap;

use crate::engine::language::Language;
use crate::metrics::KeyStats;

/// English letter progression order, mirroring keybr.com. Kept as a named
/// constant because the English tests are written against it; other code
/// asks the scheduler's language (`Language::unlock_order`).
///
/// The order drives both:
///   * the **starter set** — the first `STARTER_COUNT` letters are unlocked at launch.
///   * the **unlock order** — subsequent letters are added one at a time, left-to-right,
///     as the user reaches the target WPM (by historical best) for every active key.
///
/// The focused key is independent of this order: it's the active letter with the
/// lowest historical-best confidence still below target (this order only breaks ties).
#[cfg(test)]
pub const UNLOCK_ORDER: &[char] = Language::English.unlock_order_const();

const STARTER_COUNT: usize = 6;

/// Number of letters force-included beyond the starter set for a given
/// `alphabet_size` ∈ [0.0, 1.0] — keybr's `maxSize − minSize`, where
/// `maxSize = minSize + round((letters − minSize) * alphabetSize)`.
pub fn forced_extra_letters(language: Language, alphabet_size: f64) -> usize {
    ((language.unlock_order().len() - STARTER_COUNT) as f64 * alphabet_size.clamp(0.0, 1.0)).round()
        as usize
}

pub struct LetterScheduler {
    /// Whose alphabet and unlock order this scheduler walks.
    language: Language,
    pub active_keys: Vec<char>,
    unlock_index: usize, // index of next letter to potentially unlock
    /// Fraction of the non-starter alphabet to force-include regardless of
    /// confidence, mirroring keybr's `alphabetSize` lesson setting.
    /// 0.0 (default) keeps the pure earn-by-confidence progression; 1.0
    /// includes every letter from the start.
    pub alphabet_size: f64,
    /// The key the generator must inject into every word.
    /// The active key with the lowest historical-best confidence below 1.0;
    /// `None` once every active key has graduated (no boosted letter, like
    /// keybr.com).
    pub focused_key: Option<char>,
}

impl LetterScheduler {
    pub fn new() -> Self {
        Self::for_language(Language::English)
    }

    /// A fresh scheduler starting from `language`'s starter set.
    pub fn for_language(language: Language) -> Self {
        let active_keys = language.unlock_order()[..STARTER_COUNT].to_vec();
        LetterScheduler {
            language,
            active_keys,
            unlock_index: STARTER_COUNT,
            alphabet_size: 0.0,
            focused_key: None,
        }
    }

    /// Align `unlock_index` with the currently-active keys.
    ///
    /// Used when rehydrating state from disk. Sets `unlock_index` to the smallest
    /// `i` such that `UNLOCK_ORDER[i]` is **not** already in `active_keys`. This
    /// matters when upgrading from an older save format whose unlock order
    /// differed: naively setting `unlock_index = active_keys.len()` would cause
    /// duplicate unlocks once `update` ran (e.g. an old save with `['e','t','a',
    /// 'o','i','n','s']` would push `UNLOCK_ORDER[7] = 'o'`, already present).
    pub fn set_unlock_index_from_active(&mut self) {
        let order = self.language.unlock_order();
        let active: std::collections::HashSet<char> = self.active_keys.iter().copied().collect();
        self.unlock_index = order
            .iter()
            .position(|c| !active.contains(c))
            .unwrap_or(order.len());
    }

    /// Restore a saved unlocked set, keeping only letters of this
    /// scheduler's language (a hand-copied or stale stats file must not
    /// smuggle foreign letters into the heatmap or the generator).
    /// Returns false, leaving the starter set in place, when fewer than
    /// the starter count of valid letters remain.
    pub fn restore_active_keys(&mut self, saved: Vec<char>) -> bool {
        let mut keys: Vec<char> = Vec::with_capacity(saved.len());
        for c in saved {
            if self.language.is_letter(c) && !keys.contains(&c) {
                keys.push(c);
            }
        }
        if keys.len() < STARTER_COUNT {
            return false;
        }
        self.active_keys = keys;
        self.set_unlock_index_from_active();
        true
    }

    /// Refresh the active key set and pick the next focused key.
    ///
    /// Mirrors keybr.com's two-phase scheduler:
    ///
    /// * **Include / unlock gate** — uses `best_confidence` so a key, once
    ///   learned, never re-locks after a bad session. A new key is unlocked
    ///   only when every currently-active key has `best_confidence >= 1.0`.
    /// * **Focus** — among active keys with `best_confidence < 1.0`, pick
    ///   the one with the *lowest* confidence (keybr sorts weakest-first in
    ///   `GuidedLesson.update`). Unpracticed keys (best 0.0) qualify
    ///   naturally. When every active key has graduated there is no focused
    ///   key, and the generator places no per-word constraint.
    pub fn update(&mut self, stats: &HashMap<char, KeyStats>, target_cpm: f64) {
        // FORCE-INCLUDE gate: mirror keybr's `alphabetSize` — every letter in
        // `unlock_order[..maxSize]` is included regardless of confidence,
        // where maxSize = STARTER_COUNT + round((letters − STARTER_COUNT) ×
        // alphabet_size). Only the extras beyond the starter set are forced
        // here: `new()` always includes the starters, and skipping them keeps
        // alphabet_size = 0.0 a strict no-op (e.g. for old-save migrations).
        // Deliberate deviation from keybr: once included, letters never
        // re-lock when the setting is lowered, matching this codebase's
        // no-relock philosophy for earned letters.
        let order = self.language.unlock_order();
        let max_size = STARTER_COUNT + forced_extra_letters(self.language, self.alphabet_size);
        let mut forced_any = false;
        for &letter in &order[STARTER_COUNT..max_size.min(order.len())] {
            if !self.active_keys.contains(&letter) {
                self.active_keys.push(letter);
                forced_any = true;
            }
        }
        if forced_any {
            // Keep `unlock_index` consistent with the (possibly extended)
            // active set so the earn gate below can't duplicate an unlock.
            self.set_unlock_index_from_active();
        }

        // INCLUDE gate: are all active keys "learned" by their historical best?
        let all_learned = self.active_keys.iter().all(|key| {
            stats
                .get(key)
                .map(|s| s.best_confidence(target_cpm) >= 1.0)
                .unwrap_or(false)
        });

        // Unlock next key if all current keys are learned (by best).
        // Defensively skip any unlock-order entries already present in
        // active_keys — this can happen after migrating an old save whose
        // unlock order differed from the current spec.
        if all_learned {
            while self.unlock_index < order.len()
                && self.active_keys.contains(&order[self.unlock_index])
            {
                self.unlock_index += 1;
            }
            if self.unlock_index < order.len() {
                let next = order[self.unlock_index];
                self.active_keys.push(next);
                self.unlock_index += 1;
            }
        }

        // FOCUS: the active key with the lowest best confidence below 1.0.
        // `active_keys` is in frequency order and `min_by` keeps the first
        // minimum, matching keybr's stable weakest-first sort on ties
        // (unpracticed keys all sit at 0.0). None once all have graduated.
        self.focused_key = self
            .active_keys
            .iter()
            .map(|&key| {
                let conf = stats
                    .get(&key)
                    .map(|s| s.best_confidence(target_cpm))
                    .unwrap_or(0.0);
                (key, conf)
            })
            .filter(|&(_, conf)| conf < 1.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(key, _)| key);
    }
}

impl Default for LetterScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_learned_stats(target_cpm: f64) -> KeyStats {
        // Create stats where confidence >= 1.0
        // confidence = speed_to_time(cpm) / filtered_time
        // speed_to_time(175) ≈ 342.86
        // So filtered_time must be <= 342.86 for confidence >= 1.0
        let mut stats = KeyStats::default();
        let fast_time = 200u64; // 200ms is well under 342ms
        for _ in 0..15 {
            stats.record_hit(fast_time);
        }
        stats.finish_lesson();
        let _ = target_cpm;
        stats
    }

    fn make_slow_stats() -> KeyStats {
        let mut stats = KeyStats::default();
        for _ in 0..15 {
            stats.record_hit(600); // 600ms — slow
        }
        stats.finish_lesson();
        stats
    }

    #[test]
    fn starts_with_six_keys() {
        let sched = LetterScheduler::new();
        assert_eq!(sched.active_keys.len(), 6);
        assert_eq!(sched.active_keys, vec!['e', 'n', 'i', 'a', 'r', 'l']);
    }

    #[test]
    fn no_unlock_without_stats() {
        let mut sched = LetterScheduler::new();
        let stats = HashMap::new();
        sched.update(&stats, 175.0);
        assert_eq!(sched.active_keys.len(), 6);
    }

    #[test]
    fn unlocks_next_letter_when_all_learned() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 7);
        assert!(sched.active_keys.contains(&'t'));
    }

    #[test]
    fn focused_key_is_weakest() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Make 5 of 6 starters learned (best >= 1.0) — 'n' is slow on both
        // current and best, so its best_confidence < 1.0 and Phase 1 picks it.
        for &key in &['e', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }
        stats.insert('n', make_slow_stats());

        sched.update(&stats, target_cpm);
        // Walking UNLOCK_ORDER = [e, n, i, a, r, l, ...]:
        //   e → best ≥ 1.0, skip
        //   n → best < 1.0, pick
        assert_eq!(sched.focused_key, Some('n'));
    }

    #[test]
    fn focused_key_is_unpracticed_key() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Only practice some starter keys — unpracticed ones have best 0.0.
        for &key in &['e', 'n', 'i'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }
        // 'a', 'r', 'l' have no stats → best_confidence 0.0 → Phase 1 picks
        // the first of them in UNLOCK_ORDER, which is 'a'.

        sched.update(&stats, target_cpm);
        // All three unpracticed keys tie at confidence 0.0 — the stable
        // tie-break keeps the first in frequency order, 'a'.
        assert_eq!(sched.focused_key, Some('a'));
    }

    #[test]
    fn include_uses_best_confidence_not_current() {
        // A key with regressed current time but a fast historical best should
        // remain "learned" for the include gate — so an unlock can still happen.
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let target_time = 60_000.0 / target_cpm; // ≈ 342.86
        let mut stats = HashMap::new();

        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            let mut ks = KeyStats::default();
            ks.attempts = 30;
            // Current is regressed (slow) — confidence < 1.0
            ks.filtered_time_ms = 2.0 * target_time;
            // But best is fast — best_confidence > 1.0
            ks.best_filtered_time_ms = 0.9 * target_time;
            stats.insert(key, ks);
        }

        sched.update(&stats, target_cpm);
        // Despite current being slow, the include gate uses best → unlock fires.
        assert_eq!(sched.active_keys.len(), 7);
        assert!(sched.active_keys.contains(&'t'));
    }

    #[test]
    fn focus_picks_lowest_confidence_not_unlock_order() {
        // Starters e, n, i, a, r, l are all active. 'i' and 'a' are both
        // below target, but 'a' is weaker — keybr focuses the weakest key,
        // not the first below-target letter in frequency order ('i').
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let target_time = 60_000.0 / target_cpm;
        let mut stats = HashMap::new();

        for &key in &['e', 'n', 'r', 'l'] {
            let mut ks = KeyStats::default();
            ks.attempts = 30;
            ks.filtered_time_ms = 0.8 * target_time;
            ks.best_filtered_time_ms = 0.8 * target_time;
            stats.insert(key, ks);
        }
        for (key, slowdown) in [('i', 2.0), ('a', 3.0)] {
            let mut ks = KeyStats::default();
            ks.attempts = 30;
            ks.filtered_time_ms = slowdown * target_time;
            ks.best_filtered_time_ms = slowdown * target_time;
            stats.insert(key, ks);
        }

        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 6, "no unlock should fire here");
        assert_eq!(sched.focused_key, Some('a'));
    }

    #[test]
    fn focus_is_none_when_all_learned_by_best() {
        // keybr's GuidedLesson only boosts a letter while some included key
        // has bestConfidence < 1 — once every active key has graduated by
        // best there is NO focused key, even if a key's current time has
        // regressed. (Re-focusing on current weakness is keybr's non-default
        // `recoverKeys` setting, which we don't implement.)
        let mut sched = LetterScheduler::new();
        sched.active_keys = UNLOCK_ORDER.to_vec();
        sched.set_unlock_index_from_active();

        let target_cpm = 175.0;
        let target_time = 60_000.0 / target_cpm;
        let mut stats = HashMap::new();

        // Every active key fast on both current and best…
        for &key in UNLOCK_ORDER {
            let mut ks = KeyStats::default();
            ks.attempts = 30;
            ks.filtered_time_ms = 0.8 * target_time; // current learned
            ks.best_filtered_time_ms = 0.8 * target_time; // best learned
            stats.insert(key, ks);
        }
        // …except 'n', whose current time has regressed badly. Its best is
        // still under target, so it must NOT be re-focused.
        let n_stats = stats.get_mut(&'n').unwrap();
        n_stats.filtered_time_ms = 3.0 * target_time; // current: worst
        n_stats.best_filtered_time_ms = 0.5 * target_time; // best: fastest

        sched.update(&stats, target_cpm);

        // Active set unchanged (every letter already unlocked, none left).
        assert_eq!(sched.active_keys.len(), UNLOCK_ORDER.len());
        assert_eq!(sched.focused_key, None);
    }

    #[test]
    fn newly_unlocked_key_becomes_focus() {
        // When all starters graduate and 't' unlocks, the fresh key has no
        // stats (confidence 0.0) — it is immediately the weakest and gets
        // the focus, exactly like keybr's brand-new letters.
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();
        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert!(sched.active_keys.contains(&'t'));
        assert_eq!(sched.focused_key, Some('t'));
    }

    #[test]
    fn sequential_unlocks() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Make all starter keys learned
        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert!(sched.active_keys.contains(&'t'));

        // Without making 't' learned, no further unlock
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 7);

        // Make 't' learned too
        stats.insert('t', make_learned_stats(target_cpm));
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 8);
        assert!(sched.active_keys.contains(&'o'));
    }

    #[test]
    fn alphabet_size_zero_changes_nothing() {
        // The default setting must leave the earn-by-confidence progression
        // untouched: no force-includes with or without stats.
        let mut sched = LetterScheduler::new();
        assert_eq!(sched.alphabet_size, 0.0);

        let stats = HashMap::new();
        sched.update(&stats, 175.0);
        assert_eq!(sched.active_keys, UNLOCK_ORDER[..STARTER_COUNT].to_vec());
    }

    #[test]
    fn alphabet_size_half_forces_sixteen_letters() {
        // maxSize = 6 + round((26 − 6) × 0.5) = 16 — forced regardless of
        // confidence, in UNLOCK_ORDER order, even with no stats at all.
        let mut sched = LetterScheduler::new();
        sched.alphabet_size = 0.5;

        let stats = HashMap::new();
        sched.update(&stats, 175.0);

        assert_eq!(sched.active_keys.len(), 16);
        assert_eq!(sched.active_keys, UNLOCK_ORDER[..16].to_vec());
    }

    #[test]
    fn forced_letters_do_not_break_earn_gate() {
        // alphabet_size 0.1 forces 2 extra letters ('t', 'o'). Even with all
        // starters learned, the next *earned* unlock ('s') must wait until
        // the forced, unpracticed letters are learned too.
        let mut sched = LetterScheduler::new();
        sched.alphabet_size = 0.1;
        let target_cpm = 175.0;
        let mut stats = HashMap::new();
        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        // Forced letters included, but no earned unlock: 't'/'o' are
        // unpracticed, so the active set isn't all-learned.
        assert_eq!(sched.active_keys, UNLOCK_ORDER[..8].to_vec());

        // Once the forced letters are learned as well, the earn gate opens.
        stats.insert('t', make_learned_stats(target_cpm));
        stats.insert('o', make_learned_stats(target_cpm));
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 9);
        assert!(sched.active_keys.contains(&'s'));
    }

    #[test]
    fn no_duplicates_when_forced_letters_already_earned() {
        // 't' and 'o' were earned the normal way; raising alphabet_size to
        // cover them (and more) must not re-add them.
        let mut sched = LetterScheduler::new();
        sched.active_keys = UNLOCK_ORDER[..8].to_vec(); // starters + t, o
        sched.set_unlock_index_from_active();
        sched.alphabet_size = 0.25; // maxSize = 6 + round(20 × 0.25) = 11

        let stats = HashMap::new();
        sched.update(&stats, 175.0);

        assert_eq!(sched.active_keys, UNLOCK_ORDER[..11].to_vec());
        let mut sorted = sched.active_keys.clone();
        sorted.sort();
        let mut dedup = sorted.clone();
        dedup.dedup();
        assert_eq!(sorted, dedup, "no duplicate force-includes");
    }

    #[test]
    fn forced_unpracticed_letter_becomes_focus() {
        // The focus pass runs over the whole active set, forced letters
        // included: a forced letter with no stats sits at confidence 0.0 and
        // immediately becomes the weakest (focused) key.
        let mut sched = LetterScheduler::new();
        sched.alphabet_size = 0.05; // maxSize = 6 + round(20 × 0.05) = 7 → 't'
        let target_cpm = 175.0;
        let mut stats = HashMap::new();
        for &key in &['e', 'n', 'i', 'a', 'r', 'l'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert!(sched.active_keys.contains(&'t'));
        assert_eq!(sched.focused_key, Some('t'));
    }

    #[test]
    fn forced_extra_letters_formula() {
        let forced_extra_letters = |a| forced_extra_letters(Language::English, a);
        assert_eq!(forced_extra_letters(0.0), 0);
        assert_eq!(forced_extra_letters(0.05), 1);
        assert_eq!(forced_extra_letters(0.5), 10);
        assert_eq!(forced_extra_letters(1.0), 20);
        // Out-of-range inputs are clamped.
        assert_eq!(forced_extra_letters(-0.5), 0);
        assert_eq!(forced_extra_letters(2.0), 20);
    }

    #[test]
    fn set_unlock_index_handles_old_order_save() {
        // Simulate a v0.2.x save written under the OLD unlock order
        // (etaoin + s). Under the NEW order, naively setting unlock_index
        // to active_keys.len() (7) would push UNLOCK_ORDER[7] = 'o' — a
        // duplicate. The fix is to advance past every entry already present.
        let mut sched = LetterScheduler::new();
        sched.active_keys = vec!['e', 't', 'a', 'o', 'i', 'n', 's'];
        sched.set_unlock_index_from_active();

        let target_cpm = 175.0;
        let mut stats = HashMap::new();
        for &key in &['e', 't', 'a', 'o', 'i', 'n', 's'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);

        assert_eq!(sched.active_keys.len(), 8, "exactly one new letter added");
        // All entries unique — no duplicates introduced.
        let mut sorted = sched.active_keys.clone();
        sorted.sort();
        let mut dedup = sorted.clone();
        dedup.dedup();
        assert_eq!(sorted, dedup, "no duplicate unlocks");
        // The 8th key is the first UNLOCK_ORDER entry not previously present.
        let new_key = *sched.active_keys.last().unwrap();
        assert!(!['e', 't', 'a', 'o', 'i', 'n', 's'].contains(&new_key));
        assert!(UNLOCK_ORDER.contains(&new_key));
    }

    #[test]
    fn turkish_scheduler_starts_and_unlocks_in_turkish_order() {
        let mut sched = LetterScheduler::for_language(Language::Turkish);
        assert_eq!(sched.active_keys, vec!['a', 'e', 'i', 'n', 'ı', 'r']);
        let target_cpm = 175.0;
        let mut stats = HashMap::new();
        for &key in &sched.active_keys.clone() {
            stats.insert(key, make_learned_stats(target_cpm));
        }
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.last(), Some(&'l'));
        assert_eq!(forced_extra_letters(Language::Turkish, 1.0), 23);
    }

    #[test]
    fn restore_drops_letters_from_another_alphabet() {
        let mut sched = LetterScheduler::for_language(Language::Turkish);
        // An English save: 'w' and 'x' are not Turkish letters.
        let saved = vec!['a', 'e', 'i', 'n', 'ı', 'r', 'w', 'x', 'l'];
        assert!(sched.restore_active_keys(saved));
        assert_eq!(sched.active_keys, vec!['a', 'e', 'i', 'n', 'ı', 'r', 'l']);
        // Too few valid letters: keep the starter set.
        let mut fresh = LetterScheduler::for_language(Language::Turkish);
        assert!(!fresh.restore_active_keys(vec!['q', 'w', 'x']));
        assert_eq!(fresh.active_keys.len(), 6);
    }
}
