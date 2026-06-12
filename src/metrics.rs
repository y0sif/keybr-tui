use crate::engine::confidence;

/// Per-key performance data used by the adaptive scheduler.
#[derive(Default, Clone)]
pub struct KeyStats {
    pub attempts: u32,
    pub errors: u32,
    /// Most recent reaction times in ms (capped at last 20 samples).
    pub reaction_times_ms: Vec<u64>,
    /// Exponentially smoothed reaction time (alpha = 0.1), updated once per
    /// lesson with the lesson's mean latency — keybr.com applies its filter
    /// per result, not per keystroke.
    pub filtered_time_ms: f64,
    /// Historical minimum of the filtered time.
    pub best_filtered_time_ms: f64,
    /// Sum of valid reaction times recorded during the current lesson.
    /// Folded into `filtered_time_ms` by `finish_lesson()`; never persisted.
    lesson_sum_ms: f64,
    /// Number of samples behind `lesson_sum_ms`.
    lesson_samples: u32,
}

impl KeyStats {
    const ALPHA: f64 = 0.1;

    pub fn record_hit(&mut self, reaction_ms: u64) {
        self.attempts += 1;
        self.reaction_times_ms.push(reaction_ms);
        if self.reaction_times_ms.len() > 20 {
            self.reaction_times_ms.remove(0);
        }
        self.lesson_sum_ms += reaction_ms as f64;
        self.lesson_samples += 1;
    }

    /// Fold the finished lesson's mean latency into the smoothed time.
    /// Mirrors keybr.com's `MutableKeyStats.append`, which runs the EMA once
    /// per result on that session's mean `timeToType` for the key.
    /// No-op for keys with no valid samples this lesson.
    pub fn finish_lesson(&mut self) {
        if self.lesson_samples == 0 {
            return;
        }
        let mean = self.lesson_sum_ms / self.lesson_samples as f64;
        self.lesson_sum_ms = 0.0;
        self.lesson_samples = 0;
        self.add_smoothed_sample(mean);
    }

    /// Apply one EMA update (alpha = 0.1, seeded verbatim by the first
    /// sample) and refresh the historical best. Also used by `--import`
    /// replay, where each keybr.com result contributes its per-session mean
    /// as one sample.
    pub fn add_smoothed_sample(&mut self, sample_ms: f64) {
        if self.filtered_time_ms == 0.0 {
            self.filtered_time_ms = sample_ms;
        } else {
            self.filtered_time_ms =
                Self::ALPHA * sample_ms + (1.0 - Self::ALPHA) * self.filtered_time_ms;
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

    /// Record a full lesson's worth of hits and close the lesson.
    fn lesson(stats: &mut KeyStats, times: &[u64]) {
        for &t in times {
            stats.record_hit(t);
        }
        stats.finish_lesson();
    }

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

    // --- Per-lesson exponential smoothing and confidence ---

    #[test]
    fn record_hit_alone_does_not_touch_filtered_time() {
        // The EMA only moves at lesson boundaries (keybr parity).
        let mut stats = KeyStats::default();
        stats.record_hit(400);
        assert!((stats.filtered_time_ms - 0.0).abs() < f64::EPSILON);
        assert!((stats.best_filtered_time_ms - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn first_lesson_seeds_filtered_time() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[400]);
        assert!((stats.filtered_time_ms - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn finish_lesson_uses_lesson_mean() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[200, 400]);
        // mean(200, 400) = 300 seeds the filter
        assert!((stats.filtered_time_ms - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn finish_lesson_without_samples_is_noop() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[400]);
        stats.finish_lesson(); // no hits since last lesson
        assert!((stats.filtered_time_ms - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn exponential_smoothing_applies_across_lessons() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[400]);
        // filtered = 400.0
        lesson(&mut stats, &[200]);
        // filtered = 0.1 * 200 + 0.9 * 400 = 20 + 360 = 380
        assert!((stats.filtered_time_ms - 380.0).abs() < 0.01);
    }

    #[test]
    fn lesson_accumulator_resets_between_lessons() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[400]);
        lesson(&mut stats, &[200]);
        // If the accumulator leaked, lesson 2's mean would be 300 and
        // filtered would be 0.1*300 + 0.9*400 = 390 instead of 380.
        assert!((stats.filtered_time_ms - 380.0).abs() < 0.01);
    }

    #[test]
    fn best_filtered_time_tracks_minimum() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[400]);
        assert!((stats.best_filtered_time_ms - 400.0).abs() < f64::EPSILON);

        // Keep practicing with fast times — filtered decreases
        for _ in 0..50 {
            lesson(&mut stats, &[200]);
        }
        // filtered_time should be close to 200 after many lessons
        assert!(stats.best_filtered_time_ms <= stats.filtered_time_ms + 0.01);
        assert!(stats.best_filtered_time_ms < 400.0);
    }

    #[test]
    fn best_does_not_regress_when_slowing_down() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[200]);
        assert!((stats.best_filtered_time_ms - 200.0).abs() < f64::EPSILON);
        lesson(&mut stats, &[800]);
        // filtered regressed, best stays at its minimum
        assert!(stats.filtered_time_ms > 200.0);
        assert!((stats.best_filtered_time_ms - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn add_smoothed_sample_matches_keybr_filter() {
        // Direct EMA path used by import replay: seed, then alpha-blend.
        let mut stats = KeyStats::default();
        stats.add_smoothed_sample(500.0);
        assert!((stats.filtered_time_ms - 500.0).abs() < f64::EPSILON);
        stats.add_smoothed_sample(300.0);
        // 0.1 * 300 + 0.9 * 500 = 480
        assert!((stats.filtered_time_ms - 480.0).abs() < 0.01);
        assert!((stats.best_filtered_time_ms - 480.0).abs() < 0.01);
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
        lesson(&mut stats, &[200]);
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
        lesson(&mut stats, &[600]);
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
        let stats = KeyStats {
            filtered_time_ms: 2.0 * target_time,      // current: regressed
            best_filtered_time_ms: 0.9 * target_time, // best: faster than target
            attempts: 50,
            ..KeyStats::default()
        };

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
        let stats = KeyStats {
            filtered_time_ms: 343.0,
            ..KeyStats::default()
        };
        let wpm = stats.wpm().expect("should have a value");
        assert!((wpm - 35.0).abs() < 0.1, "expected ~35 WPM, got {}", wpm);
    }

    #[test]
    fn best_wpm_converts_best_filtered_time_to_wpm() {
        let stats = KeyStats {
            best_filtered_time_ms: 343.0,
            ..KeyStats::default()
        };
        let wpm = stats.best_wpm().expect("should have a value");
        assert!((wpm - 35.0).abs() < 0.1, "expected ~35 WPM, got {}", wpm);
    }

    #[test]
    fn is_proficient_uses_confidence() {
        let mut stats = KeyStats::default();
        lesson(&mut stats, &[200]); // Fast — confidence > 1.0
        assert!(stats.is_proficient(175.0));

        let mut slow_stats = KeyStats::default();
        lesson(&mut slow_stats, &[600]); // Slow — confidence < 1.0
        assert!(!slow_stats.is_proficient(175.0));
    }
}
