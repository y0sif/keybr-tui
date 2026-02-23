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

    #[allow(dead_code)]
    pub fn all_unlocked(&self) -> bool {
        self.unlock_index >= UNLOCK_ORDER.len()
    }
}

impl Default for LetterScheduler {
    fn default() -> Self {
        Self::new()
    }
}
