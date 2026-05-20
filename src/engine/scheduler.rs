use std::collections::HashMap;

use crate::metrics::KeyStats;

/// Fixed letter progression order, mirroring keybr.com.
///
/// This drives both:
///   * the **starter set** — the first `STARTER_COUNT` letters are unlocked at launch.
///   * the **unlock order** — subsequent letters are added one at a time, left-to-right,
///     as the user reaches the target WPM (by historical best) for every active key.
///
/// The focused key is also chosen by walking this order: the first active letter that
/// hasn't yet hit the target speed (by historical best) is what we practice next.
pub const UNLOCK_ORDER: &[char] = &[
    'e', 'n', 'i', 'a', 'r', 'l', // starter set (6)
    't', 'o', 's', 'u', 'd', 'y', 'c', 'g', 'h', 'p', 'm', 'k', 'b', 'w', 'f', 'z', 'v', 'x', 'q',
    'j',
];

const STARTER_COUNT: usize = 6;

pub struct LetterScheduler {
    pub active_keys: Vec<char>,
    unlock_index: usize, // index of next letter to potentially unlock
    /// The key the generator must inject into every word.
    /// Picked by walking `UNLOCK_ORDER` and taking the first active key whose
    /// historical best confidence is still below target; falls back to current
    /// weakest once every active key has graduated.
    pub focused_key: Option<char>,
}

impl LetterScheduler {
    pub fn new() -> Self {
        let active_keys = UNLOCK_ORDER[..STARTER_COUNT].to_vec();
        LetterScheduler {
            active_keys,
            unlock_index: STARTER_COUNT,
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
        let active: std::collections::HashSet<char> = self.active_keys.iter().copied().collect();
        self.unlock_index = UNLOCK_ORDER
            .iter()
            .position(|c| !active.contains(c))
            .unwrap_or(UNLOCK_ORDER.len());
    }

    /// Refresh the active key set and pick the next focused key.
    ///
    /// Mirrors keybr.com's two-phase scheduler:
    ///
    /// * **Include / unlock gate** — uses `best_confidence` so a key, once
    ///   learned, never re-locks after a bad session. A new key is unlocked
    ///   only when every currently-active key has `best_confidence >= 1.0`.
    /// * **Focus phase 1 (fixed order)** — walk `UNLOCK_ORDER` left-to-right
    ///   and return the first active key whose `best_confidence` is still
    ///   below 1.0. Unpracticed keys (best 0.0) qualify naturally.
    /// * **Focus phase 2 (fallback)** — only when every active key has
    ///   graduated by best: pick the active key with the lowest *current*
    ///   confidence, so maintenance practice tracks present weakness.
    pub fn update(&mut self, stats: &HashMap<char, KeyStats>, target_cpm: f64) {
        // INCLUDE gate: are all active keys "learned" by their historical best?
        let all_learned = self.active_keys.iter().all(|key| {
            stats
                .get(key)
                .map(|s| s.best_confidence(target_cpm) >= 1.0)
                .unwrap_or(false)
        });

        // Unlock next key if all current keys are learned (by best).
        // Defensively skip any UNLOCK_ORDER entries already present in
        // active_keys — this can happen after migrating an old save whose
        // unlock order differed from the current spec.
        if all_learned {
            while self.unlock_index < UNLOCK_ORDER.len()
                && self.active_keys.contains(&UNLOCK_ORDER[self.unlock_index])
            {
                self.unlock_index += 1;
            }
            if self.unlock_index < UNLOCK_ORDER.len() {
                let next = UNLOCK_ORDER[self.unlock_index];
                self.active_keys.push(next);
                self.unlock_index += 1;
            }
        }

        // Build a set of active keys for fast lookup during the focus walk.
        let active: std::collections::HashSet<char> = self.active_keys.iter().copied().collect();

        // FOCUS phase 1: walk UNLOCK_ORDER, take the first active key whose
        // historical best is still below target.
        let phase1 = UNLOCK_ORDER.iter().copied().find(|c| {
            if !active.contains(c) {
                return false;
            }
            let best_conf = stats
                .get(c)
                .map(|s| s.best_confidence(target_cpm))
                .unwrap_or(0.0);
            best_conf < 1.0
        });

        self.focused_key = if let Some(key) = phase1 {
            Some(key)
        } else {
            // FOCUS phase 2: every active key has graduated by best — fall back
            // to "current weakest" using live (not best) confidence.
            self.active_keys
                .iter()
                .map(|&key| {
                    let conf = stats
                        .get(&key)
                        .map(|s| s.confidence(target_cpm))
                        .unwrap_or(0.0);
                    (key, conf)
                })
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(key, _)| key)
        };
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
        let _ = target_cpm;
        stats
    }

    fn make_slow_stats() -> KeyStats {
        let mut stats = KeyStats::default();
        for _ in 0..15 {
            stats.record_hit(600); // 600ms — slow
        }
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
        assert!(sched.focused_key.is_some());
        let focused = sched.focused_key.unwrap();
        assert!(
            ['a', 'r', 'l'].contains(&focused),
            "focused key should be an unpracticed key, got '{}'",
            focused
        );
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
    fn focus_walks_keybr_order() {
        // Starters e, n, i, a, r, l are all active. Set e, n, r, l learned by
        // best; leave i and a not learned by best. Walking UNLOCK_ORDER:
        //   e → best ≥ 1.0, skip
        //   n → best ≥ 1.0, skip
        //   i → best < 1.0, pick
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
        for &key in &['i', 'a'] {
            let mut ks = KeyStats::default();
            ks.attempts = 30;
            ks.filtered_time_ms = 2.0 * target_time;
            ks.best_filtered_time_ms = 2.0 * target_time;
            stats.insert(key, ks);
        }

        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 6, "no unlock should fire here");
        assert_eq!(sched.focused_key, Some('i'));
    }

    #[test]
    fn focus_falls_back_to_current_when_all_learned_by_best() {
        // Every active key has best_confidence >= 1.0, so Phase 1 returns None
        // and Phase 2 picks the active key with the lowest CURRENT confidence,
        // even if it has a great historical best.
        //
        // To prevent the unlock gate from continuously pulling in fresh keys
        // (which would re-arm Phase 1 because the newly unlocked key has
        // best 0.0), we unlock ALL letters up-front and feed stats for every
        // single one — so the unlock cursor sits at the end of UNLOCK_ORDER.
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
        // …except 'n', which has a great historical best (so Phase 1 won't
        // pick it) but a regressed current time — the worst current of the lot.
        let n_stats = stats.get_mut(&'n').unwrap();
        n_stats.filtered_time_ms = 3.0 * target_time; // current: worst
        n_stats.best_filtered_time_ms = 0.5 * target_time; // best: fastest

        sched.update(&stats, target_cpm);

        // Active set unchanged (every letter already unlocked, none left).
        assert_eq!(sched.active_keys.len(), UNLOCK_ORDER.len());
        // Phase 1 returns None (all best >= 1.0), Phase 2 picks worst current.
        assert_eq!(sched.focused_key, Some('n'));
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
}
