use std::collections::HashMap;

use crate::metrics::KeyStats;

use super::phonetics::{letter_index, BIGRAMS};

/// Generate a batch of pseudo-words (~60 chars) using the active key set.
///
/// Uses a bigram Markov chain: each character is sampled based on what
/// commonly follows the previous character in English, filtered to
/// only include letters in `active_keys`.
///
/// `stats` is used to bias generation toward weaker keys (keys with lower
/// proficiency get a higher selection weight to force more practice).
pub fn generate_text(active_keys: &[char], stats: &HashMap<char, KeyStats>) -> String {
    let mut rng = SimpleRng::new();
    let mut result = String::with_capacity(64);

    // Build bias weights for each active key (weaker key = higher weight)
    let bias: Vec<(char, u32)> = active_keys
        .iter()
        .map(|&c| {
            let weight = stats
                .get(&c)
                .map(|s| weakness_weight(s))
                .unwrap_or(100); // untouched keys get a moderate boost
            (c, weight)
        })
        .collect();

    while result.len() < 55 {
        // Generate one word of 3–8 chars
        let word = generate_word(active_keys, &bias, &mut rng);
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&word);
    }

    result
}

fn generate_word(
    active_keys: &[char],
    bias: &[(char, u32)],
    rng: &mut SimpleRng,
) -> String {
    let target_len = 3 + (rng.next_u32() % 6) as usize; // 3–8 chars
    let mut word = String::with_capacity(target_len);

    // Pick starting letter — weighted by bias toward weak keys
    let first = weighted_sample(bias, rng);
    word.push(first);

    let mut prev = first;

    for _ in 1..target_len {
        let next = next_char(prev, active_keys, bias, rng);
        word.push(next);
        prev = next;
    }

    word
}

/// Pick the next character using the bigram table, filtered to active keys,
/// then mixed with the weakness bias to surface weak keys more often.
fn next_char(
    prev: char,
    active_keys: &[char],
    bias: &[(char, u32)],
    rng: &mut SimpleRng,
) -> char {
    let row = letter_index(prev);
    let bigram_row = &BIGRAMS[row];

    // Build candidate list: (char, combined_weight)
    // combined = bigram_freq * weakness_bias (both factors matter)
    let candidates: Vec<(char, u32)> = active_keys
        .iter()
        .filter_map(|&c| {
            let bigram_weight = bigram_row[letter_index(c)] as u32;
            if bigram_weight == 0 {
                return None; // this transition never happens in English
            }
            let weak_weight = bias
                .iter()
                .find(|(bc, _)| *bc == c)
                .map(|(_, w)| *w)
                .unwrap_or(50);
            // Scale: bigram contributes 70%, weakness 30%
            let combined = (bigram_weight * 7 + weak_weight * 3) / 10;
            Some((c, combined.max(1)))
        })
        .collect();

    if candidates.is_empty() {
        // Fallback: any active key, equal weight
        let i = (rng.next_u32() as usize) % active_keys.len();
        return active_keys[i];
    }

    weighted_sample(&candidates, rng)
}

/// Sample from a weighted list. Returns a char.
fn weighted_sample(weights: &[(char, u32)], rng: &mut SimpleRng) -> char {
    let total: u32 = weights.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return weights[0].0;
    }
    let mut pick = rng.next_u32() % total;
    for (c, w) in weights {
        if pick < *w {
            return *c;
        }
        pick -= w;
    }
    weights.last().unwrap().0
}

/// Convert KeyStats into a "how much should we emphasize this key" weight.
/// Keys never practiced or with poor performance get higher weights.
fn weakness_weight(stats: &KeyStats) -> u32 {
    if stats.reaction_times_ms.is_empty() {
        return 120; // never practiced: high priority
    }
    let avg = stats.avg_reaction_ms();
    let err = stats.error_rate();

    // Base weight inversely proportional to speed, boosted by errors
    let speed_score = (avg / 10.0).min(200.0) as u32;
    let error_boost = (err * 100.0) as u32;
    (speed_score + error_boost).max(10).min(300)
}

/// Simple deterministic PRNG (xorshift32) — no external rand crate needed.
pub struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    pub fn new() -> Self {
        // Seed from a non-constant source: use address of a stack var as entropy
        let seed_source: u32 = 0xDEAD_BEEF;
        // Mix in something that varies per process (stack address bits)
        let addr = &seed_source as *const u32 as u64;
        let seed = (seed_source ^ (addr as u32)).max(1);
        SimpleRng { state: seed }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }
}
