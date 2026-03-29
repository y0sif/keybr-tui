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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_key_stats_are_empty() {
        let stats = KeyStats::default();
        assert_eq!(stats.attempts, 0);
        assert_eq!(stats.errors, 0);
        assert!(stats.reaction_times_ms.is_empty());
    }

    #[test]
    fn avg_reaction_ms_no_samples_returns_max() {
        let stats = KeyStats::default();
        assert_eq!(stats.avg_reaction_ms(), f64::MAX);
    }

    #[test]
    fn avg_reaction_ms_calculates_correctly() {
        let mut stats = KeyStats::default();
        stats.record_hit(100);
        stats.record_hit(200);
        stats.record_hit(300);
        assert!((stats.avg_reaction_ms() - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn error_rate_with_no_attempts() {
        let stats = KeyStats::default();
        assert!((stats.error_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn error_rate_calculates_correctly() {
        let mut stats = KeyStats::default();
        stats.record_hit(100);
        stats.record_hit(100);
        stats.record_error(); // 1 error out of 3 attempts
        assert!((stats.error_rate() - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn not_proficient_with_few_samples() {
        let mut stats = KeyStats::default();
        for _ in 0..5 {
            stats.record_hit(100);
        }
        assert!(!stats.is_proficient(400));
    }

    #[test]
    fn proficient_with_enough_fast_samples() {
        let mut stats = KeyStats::default();
        for _ in 0..15 {
            stats.record_hit(300);
        }
        assert!(stats.is_proficient(400));
    }

    #[test]
    fn not_proficient_when_too_slow() {
        let mut stats = KeyStats::default();
        for _ in 0..15 {
            stats.record_hit(500);
        }
        assert!(!stats.is_proficient(400));
    }

    #[test]
    fn not_proficient_with_high_error_rate() {
        let mut stats = KeyStats::default();
        for _ in 0..8 {
            stats.record_hit(300);
        }
        // Add errors to push error rate above 10%
        for _ in 0..4 {
            stats.record_error();
        }
        // 4 errors / 12 attempts = 33% error rate
        assert!(!stats.is_proficient(400));
    }

    #[test]
    fn reaction_times_capped_at_20() {
        let mut stats = KeyStats::default();
        for i in 0..30 {
            stats.record_hit(i * 10);
        }
        assert_eq!(stats.reaction_times_ms.len(), 20);
    }
}
