use std::collections::HashMap;

use crate::metrics::KeyStats;

/// Letters ordered from most to least frequent in English.
/// This is the unlock order — the user starts with the first 6 and
/// progressively gains access to more as they improve.
const UNLOCK_ORDER: &[char] = &[
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
}

impl LetterScheduler {
    pub fn new() -> Self {
        let active_keys = UNLOCK_ORDER[..STARTER_COUNT].to_vec();
        LetterScheduler {
            active_keys,
            unlock_index: STARTER_COUNT,
        }
    }

    /// Check if all active keys meet proficiency. If so, unlock the next letter.
    /// Returns the newly unlocked letter, or None if no unlock happened.
    pub fn try_unlock(
        &mut self,
        stats: &HashMap<char, KeyStats>,
        target_ms: u64,
    ) -> Option<char> {
        if self.unlock_index >= UNLOCK_ORDER.len() {
            return None; // all letters unlocked
        }

        let all_proficient = self.active_keys.iter().all(|key| {
            stats
                .get(key)
                .map(|s| s.is_proficient(target_ms))
                .unwrap_or(false)
        });

        if all_proficient {
            let next = UNLOCK_ORDER[self.unlock_index];
            self.active_keys.push(next);
            self.unlock_index += 1;
            Some(next)
        } else {
            None
        }
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

    fn make_proficient_stats(target_ms: u64) -> KeyStats {
        let mut stats = KeyStats::default();
        // Need at least 10 samples with avg <= target_ms and error_rate < 0.10
        for _ in 0..15 {
            stats.record_hit(target_ms - 10);
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
    fn no_unlock_without_proficiency() {
        let mut sched = LetterScheduler::new();
        let stats = HashMap::new();
        let result = sched.try_unlock(&stats, 400);
        assert!(result.is_none());
        assert_eq!(sched.active_keys.len(), 6);
    }

    #[test]
    fn unlocks_next_letter_when_all_proficient() {
        let mut sched = LetterScheduler::new();
        let target_ms = 400;
        let mut stats = HashMap::new();

        // Make all 6 starter keys proficient
        for &key in &['e', 't', 'a', 'o', 'i', 'n'] {
            stats.insert(key, make_proficient_stats(target_ms));
        }

        let unlocked = sched.try_unlock(&stats, target_ms);
        assert_eq!(unlocked, Some('s'));
        assert_eq!(sched.active_keys.len(), 7);
        assert!(sched.active_keys.contains(&'s'));
    }

    #[test]
    fn sequential_unlocks() {
        let mut sched = LetterScheduler::new();
        let target_ms = 400;
        let mut stats = HashMap::new();

        // Make all starter keys proficient
        for &key in &['e', 't', 'a', 'o', 'i', 'n'] {
            stats.insert(key, make_proficient_stats(target_ms));
        }

        // First unlock: 's'
        let first = sched.try_unlock(&stats, target_ms);
        assert_eq!(first, Some('s'));

        // Without making 's' proficient, no further unlock
        let none = sched.try_unlock(&stats, target_ms);
        assert!(none.is_none());

        // Make 's' proficient too
        stats.insert('s', make_proficient_stats(target_ms));
        let second = sched.try_unlock(&stats, target_ms);
        assert_eq!(second, Some('r'));
    }
}
