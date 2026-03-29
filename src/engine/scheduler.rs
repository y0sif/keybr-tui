use std::collections::HashMap;

use crate::metrics::KeyStats;

/// Letters ordered from most to least frequent in English.
/// This is the unlock order — the user starts with the first 6 and
/// progressively gains access to more as they improve.
pub const UNLOCK_ORDER: &[char] = &[
    'e', 't', 'a', 'o', 'i', 'n', // starter set
    's', 'r', 'h', 'l', 'd', 'c', // next unlocks
    'u', 'm', 'f', 'p', 'g', 'w', // medium frequency
    'y', 'b', 'v', 'k', 'x', 'j', // low frequency
    'q', 'z',                      // rare
];

const STARTER_COUNT: usize = 6;

pub struct LetterScheduler {
    pub active_keys: Vec<char>,
    unlock_index: usize, // index of next letter to potentially unlock
    /// The weakest included key (lowest confidence). Must appear in every word.
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

    /// Set `unlock_index` to match the current `active_keys` length.
    /// Used when restoring state from saved stats.
    pub fn set_unlock_index_from_active(&mut self) {
        self.unlock_index = self.active_keys.len();
    }

    /// Update which keys are included based on current confidence levels.
    ///
    /// - Minimum 6 keys always included
    /// - A key with best confidence >= 1.0 stays included
    /// - New key unlocks only when ALL included keys have confidence >= 1.0
    /// - Focused key = included key with lowest confidence
    pub fn update(&mut self, stats: &HashMap<char, KeyStats>, target_cpm: f64) {
        // Check if all active keys are learned (confidence >= 1.0)
        let all_learned = self.active_keys.iter().all(|key| {
            stats
                .get(key)
                .map(|s| s.confidence(target_cpm) >= 1.0)
                .unwrap_or(false)
        });

        // Unlock next key if all current keys are learned
        if all_learned && self.unlock_index < UNLOCK_ORDER.len() {
            let next = UNLOCK_ORDER[self.unlock_index];
            self.active_keys.push(next);
            self.unlock_index += 1;
        }

        // Find the focused key: active key with lowest confidence
        self.focused_key = self
            .active_keys
            .iter()
            .map(|&key| {
                let conf = stats
                    .get(&key)
                    .map(|s| s.confidence(target_cpm))
                    .unwrap_or(0.0);
                (key, conf)
            })
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
        assert_eq!(sched.active_keys, vec!['e', 't', 'a', 'o', 'i', 'n']);
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

        for &key in &['e', 't', 'a', 'o', 'i', 'n'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 7);
        assert!(sched.active_keys.contains(&'s'));
    }

    #[test]
    fn focused_key_is_weakest() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Make all keys learned except 'n'
        for &key in &['e', 't', 'a', 'o', 'i'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }
        stats.insert('n', make_slow_stats());

        sched.update(&stats, target_cpm);
        assert_eq!(sched.focused_key, Some('n'));
    }

    #[test]
    fn focused_key_is_unpracticed_key() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Only practice some keys — unpracticed ones have confidence 0.0
        for &key in &['e', 't', 'a'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }
        // 'o', 'i', 'n' have no stats → confidence 0.0

        sched.update(&stats, target_cpm);
        // Focused should be one of the unpracticed keys
        assert!(sched.focused_key.is_some());
        let focused = sched.focused_key.unwrap();
        assert!(
            ['o', 'i', 'n'].contains(&focused),
            "focused key should be an unpracticed key, got '{}'",
            focused
        );
    }

    #[test]
    fn sequential_unlocks() {
        let mut sched = LetterScheduler::new();
        let target_cpm = 175.0;
        let mut stats = HashMap::new();

        // Make all starter keys learned
        for &key in &['e', 't', 'a', 'o', 'i', 'n'] {
            stats.insert(key, make_learned_stats(target_cpm));
        }

        sched.update(&stats, target_cpm);
        assert!(sched.active_keys.contains(&'s'));

        // Without making 's' learned, no further unlock
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 7);

        // Make 's' learned too
        stats.insert('s', make_learned_stats(target_cpm));
        sched.update(&stats, target_cpm);
        assert_eq!(sched.active_keys.len(), 8);
        assert!(sched.active_keys.contains(&'r'));
    }
}
