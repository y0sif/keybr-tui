/// Convert CPM (characters per minute) to milliseconds per character.
///
/// E.g. 175 CPM → 1000 / (175/60) ≈ 342.86 ms per character.
pub fn speed_to_time(cpm: f64) -> f64 {
    1000.0 / (cpm / 60.0)
}

/// Calculate confidence for a key.
///
/// Returns the ratio: target_time / actual_filtered_time.
/// A confidence >= 1.0 means the key is "learned" — the user types it
/// at least as fast as the target speed.
pub fn confidence(target_cpm: f64, filtered_time_to_type_ms: f64) -> f64 {
    if filtered_time_to_type_ms <= 0.0 {
        return 0.0;
    }
    speed_to_time(target_cpm) / filtered_time_to_type_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_to_time_175_cpm() {
        let t = speed_to_time(175.0);
        // 1000 / (175/60) = 60000/175 ≈ 342.857
        assert!((t - 342.857).abs() < 0.01);
    }

    #[test]
    fn speed_to_time_60_cpm() {
        let t = speed_to_time(60.0);
        // 1000 / (60/60) = 1000
        assert!((t - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn confidence_exactly_at_target() {
        // If filtered time equals target time, confidence should be 1.0
        let target_cpm = 175.0;
        let target_time = speed_to_time(target_cpm);
        let c = confidence(target_cpm, target_time);
        assert!((c - 1.0).abs() < 0.001);
    }

    #[test]
    fn confidence_faster_than_target() {
        // Faster than target → confidence > 1.0
        let c = confidence(175.0, 200.0); // 200ms is faster than 342ms target
        assert!(c > 1.0);
    }

    #[test]
    fn confidence_slower_than_target() {
        // Slower than target → confidence < 1.0
        let c = confidence(175.0, 500.0); // 500ms is slower than 342ms target
        assert!(c < 1.0);
    }

    #[test]
    fn confidence_zero_time_returns_zero() {
        assert!((confidence(175.0, 0.0) - 0.0).abs() < f64::EPSILON);
    }
}
