/// Per-key performance data used by the adaptive scheduler.
#[derive(Default, Clone)]
pub struct KeyStats {
    pub attempts: u32,
    pub errors: u32,
    /// Most recent reaction times in ms (capped at last 20 samples).
    pub reaction_times_ms: Vec<u64>,
}

impl KeyStats {
    pub fn record_hit(&mut self, reaction_ms: u64) {
        self.attempts += 1;
        self.reaction_times_ms.push(reaction_ms);
        if self.reaction_times_ms.len() > 20 {
            self.reaction_times_ms.remove(0);
        }
    }

    pub fn record_error(&mut self) {
        self.errors += 1;
        self.attempts += 1;
    }

    /// Average reaction time over the stored sample window.
    /// Returns f64::MAX if no samples yet.
    pub fn avg_reaction_ms(&self) -> f64 {
        if self.reaction_times_ms.is_empty() {
            return f64::MAX;
        }
        let sum: u64 = self.reaction_times_ms.iter().sum();
        sum as f64 / self.reaction_times_ms.len() as f64
    }

    /// Error rate over all attempts (0.0 – 1.0).
    pub fn error_rate(&self) -> f64 {
        if self.attempts == 0 {
            return 0.0;
        }
        self.errors as f64 / self.attempts as f64
    }

    /// Whether this key has met the proficiency threshold.
    /// `target_ms`: maximum allowed average reaction time.
    pub fn is_proficient(&self, target_ms: u64) -> bool {
        // Need at least 10 samples before declaring proficiency
        if self.reaction_times_ms.len() < 10 {
            return false;
        }
        self.avg_reaction_ms() <= target_ms as f64 && self.error_rate() < 0.10
    }
}
