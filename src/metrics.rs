use crate::engine::confidence;

/// Per-key performance data used by the adaptive scheduler.
#[derive(Default, Clone)]
pub struct KeyStats {
    pub attempts: u32,
    pub errors: u32,
    /// Most recent reaction times in ms (capped at last 20 samples).
    pub reaction_times_ms: Vec<u64>,
    /// Exponentially smoothed reaction time (alpha = 0.1).
    pub filtered_time_ms: f64,
    /// Historical minimum of the filtered time.
    pub best_filtered_time_ms: f64,
}

impl KeyStats {
    const ALPHA: f64 = 0.1;

    pub fn record_hit(&mut self, reaction_ms: u64) {
        self.attempts += 1;
        self.reaction_times_ms.push(reaction_ms);
        if self.reaction_times_ms.len() > 20 {
            self.reaction_times_ms.remove(0);
        }

        // Exponential smoothing
        if self.filtered_time_ms == 0.0 {
            self.filtered_time_ms = reaction_ms as f64;
        } else {
            self.filtered_time_ms =
                Self::ALPHA * reaction_ms as f64 + (1.0 - Self::ALPHA) * self.filtered_time_ms;
        }

        // Track best (historical minimum)
        if self.best_filtered_time_ms == 0.0 || self.filtered_time_ms < self.best_filtered_time_ms {
            self.best_filtered_time_ms = self.filtered_time_ms;
        }
    }

    pub fn record_error(&mut self) {
        self.errors += 1;
        self.attempts += 1;
    }

    /// Average reaction time over the stored sample window.
    /// Returns f64::MAX if no samples yet.
    #[allow(dead_code)]
    pub fn avg_reaction_ms(&self) -> f64 {
        if self.reaction_times_ms.is_empty() {
            return f64::MAX;
        }
        let sum: u64 = self.reaction_times_ms.iter().sum();
        sum as f64 / self.reaction_times_ms.len() as f64
    }

    /// Error rate over all attempts (0.0 - 1.0).
    #[allow(dead_code)]
    pub fn error_rate(&self) -> f64 {
        if self.attempts == 0 {
            return 0.0;
        }
        self.errors as f64 / self.attempts as f64
    }

    /// Confidence for this key given a target CPM.
    /// confidence >= 1.0 means the key is "learned".
    pub fn confidence(&self, target_cpm: f64) -> f64 {
        if self.filtered_time_ms == 0.0 {
            return 0.0;
        }
        confidence::confidence(target_cpm, self.filtered_time_ms)
    }

    /// Best (historical) confidence for this key given a target CPM.
    /// Mirrors `confidence()` but reads `best_filtered_time_ms` so a
    /// learned key cannot re-lock after a bad session.
    /// Returns 0.0 when no sample has been recorded yet.
    pub fn best_confidence(&self, target_cpm: f64) -> f64 {
        if self.best_filtered_time_ms == 0.0 {
            return 0.0;
        }
        confidence::confidence(target_cpm, self.best_filtered_time_ms)
    }

    /// Whether this key has met the proficiency threshold.
    /// Uses the confidence system: confidence >= 1.0 means learned.
    #[allow(dead_code)]
    pub fn is_proficient(&self, target_cpm: f64) -> bool {
        self.confidence(target_cpm) >= 1.0
    }

    /// Current smoothed WPM for this key derived from `filtered_time_ms`.
    /// Returns `None` if no sample has been recorded yet.
    ///
    /// CPM = 60_000 / ms_per_char, and WPM = CPM / 5, so
    /// WPM = 12_000 / filtered_time_ms.
    pub fn wpm(&self) -> Option<f64> {
        if self.filtered_time_ms > 0.0 {
            Some(12_000.0 / self.filtered_time_ms)
        } else {
            None
        }
    }

    /// Historical best WPM derived from `best_filtered_time_ms`.
    /// Returns `None` if no sample has been recorded yet.
    pub fn best_wpm(&self) -> Option<f64> {
        if self.best_filtered_time_ms > 0.0 {
            Some(12_000.0 / self.best_filtered_time_ms)
        } else {
            None
        }
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
        assert!((stats.filtered_time_ms - 0.0).abs() < f64::EPSILON);
        assert!((stats.best_filtered_time_ms - 0.0).abs() < f64::EPSILON);
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
    fn reaction_times_capped_at_20() {
        let mut stats = KeyStats::default();
        for i in 0..30 {
            stats.record_hit(i * 10);
        }
        assert_eq!(stats.reaction_times_ms.len(), 20);
    }

    // --- New tests for exponential smoothing and confidence ---

    #[test]
    fn first_hit_sets_filtered_time() {
        let mut stats = KeyStats::default();
        stats.record_hit(400);
        assert!((stats.filtered_time_ms - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn exponential_smoothing_applies() {
        let mut stats = KeyStats::default();
        stats.record_hit(400);
        // filtered = 400.0
        stats.record_hit(200);
        // filtered = 0.1 * 200 + 0.9 * 400 = 20 + 360 = 380
        assert!((stats.filtered_time_ms - 380.0).abs() < 0.01);
    }

    #[test]
    fn best_filtered_time_tracks_minimum() {
        let mut stats = KeyStats::default();
        stats.record_hit(400);
        assert!((stats.best_filtered_time_ms - 400.0).abs() < f64::EPSILON);

        // Keep hitting with fast times — filtered decreases
        for _ in 0..50 {
            stats.record_hit(200);
        }
        // filtered_time should be close to 200 after many samples
        assert!(stats.best_filtered_time_ms <= stats.filtered_time_ms + 0.01);
        assert!(stats.best_filtered_time_ms < 400.0);
    }

    #[test]
    fn confidence_zero_without_data() {
        let stats = KeyStats::default();
        assert!((stats.confidence(175.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn confidence_increases_with_speed() {
        let mut stats = KeyStats::default();
        // Target 175 CPM → target time ≈ 342.86ms
        // If filtered time is 200ms, confidence = 342.86 / 200 ≈ 1.71
        stats.record_hit(200);
        let c = stats.confidence(175.0);
        assert!(
            c > 1.0,
            "confidence should be > 1.0 for fast typing, got {}",
            c
        );
    }

    #[test]
    fn confidence_below_one_when_slow() {
        let mut stats = KeyStats::default();
        // If filtered time is 600ms, confidence = 342.86 / 600 ≈ 0.57
        stats.record_hit(600);
        let c = stats.confidence(175.0);
        assert!(
            c < 1.0,
            "confidence should be < 1.0 for slow typing, got {}",
            c
        );
    }

    #[test]
    fn best_confidence_zero_without_sample() {
        let stats = KeyStats::default();
        assert!((stats.best_confidence(175.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn best_confidence_uses_best_not_current() {
        // Target time @ 175 CPM ≈ 342.86ms.
        // Construct a key whose current filtered time is regressed (slow), but
        // whose best_filtered_time_ms is faster than target — best_confidence
        // should be >= 1.0 even though current confidence is < 1.0.
        let target_cpm = 175.0;
        let target_time = 60_000.0 / target_cpm; // ≈ 342.86
        let mut stats = KeyStats::default();
        stats.filtered_time_ms = 2.0 * target_time; // current: regressed
        stats.best_filtered_time_ms = 0.9 * target_time; // best: faster than target
        stats.attempts = 50;

        let cur = stats.confidence(target_cpm);
        let best = stats.best_confidence(target_cpm);
        assert!(cur < 1.0, "current confidence should be < 1.0, got {}", cur);
        assert!(
            best >= 1.0,
            "best confidence should be >= 1.0, got {}",
            best
        );
    }

    #[test]
    fn wpm_none_without_sample() {
        let stats = KeyStats::default();
        assert!(stats.wpm().is_none());
        assert!(stats.best_wpm().is_none());
    }

    #[test]
    fn wpm_converts_filtered_time_to_wpm() {
        // 343 ms/char ≈ 175 CPM ≈ 35 WPM
        let mut stats = KeyStats::default();
        stats.filtered_time_ms = 343.0;
        let wpm = stats.wpm().expect("should have a value");
        assert!((wpm - 35.0).abs() < 0.1, "expected ~35 WPM, got {}", wpm);
    }

    #[test]
    fn best_wpm_converts_best_filtered_time_to_wpm() {
        let mut stats = KeyStats::default();
        stats.best_filtered_time_ms = 343.0;
        let wpm = stats.best_wpm().expect("should have a value");
        assert!((wpm - 35.0).abs() < 0.1, "expected ~35 WPM, got {}", wpm);
    }

    #[test]
    fn is_proficient_uses_confidence() {
        let mut stats = KeyStats::default();
        stats.record_hit(200); // Fast — confidence > 1.0
        assert!(stats.is_proficient(175.0));

        let mut slow_stats = KeyStats::default();
        slow_stats.record_hit(600); // Slow — confidence < 1.0
        assert!(!slow_stats.is_proficient(175.0));
    }
}
