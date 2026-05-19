use std::collections::HashMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
}

fn default_today_minutes_practiced() -> u32 {
    0
}

fn default_today_date() -> String {
    String::new()
}

/// Compute YYYY-MM-DD for the current Unix day (UTC).
///
/// Uses Howard Hinnant's "civil_from_days" algorithm (public domain) to map
/// days-since-1970-01-01 → (year, month, day). Avoids pulling in chrono.
/// Reference: https://howardhinnant.github.io/date_algorithms.html
pub fn today_date_string() -> String {
    use std::time::SystemTime;
    let secs = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return String::new(),
    };
    date_string_from_unix_secs(secs)
}

/// Pure helper: format the UTC date for a Unix timestamp in seconds as YYYY-MM-DD.
fn date_string_from_unix_secs(secs: i64) -> String {
    // Days since 1970-01-01 (UTC). Floor division so negatives round down.
    let z: i64 = secs.div_euclid(86_400);
    // Shift epoch from 1970-01-01 → 0000-03-01 (Hinnant's "civil_from_days").
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe: u32 = (z - era * 146_097) as u32; // [0, 146096]
    let yoe: u32 = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y: i64 = yoe as i64 + era * 400;
    let doy: u32 = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp: u32 = (5 * doy + 2) / 153; // [0, 11]
    let d: u32 = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m: u32 = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year: i64 = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", year, m, d)
}

/// Per-key statistics saved to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedKeyStats {
    pub attempts: u32,
    pub errors: u32,
    pub filtered_time_ms: f64,
    pub best_filtered_time_ms: f64,
    /// Last N reaction times for context (keep 50).
    pub recent_times_ms: Vec<u64>,
}

/// All persistent stats, serialized to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedStats {
    #[serde(default = "default_version")]
    pub version: u32,

    /// Per-key statistics.
    pub keys: HashMap<char, SavedKeyStats>,

    /// Which letters have been unlocked.
    pub unlocked_letters: Vec<char>,

    /// Total lessons completed (all time).
    pub total_lessons: u32,

    /// Timestamp of last session (ISO 8601).
    pub last_session: String,

    /// Minutes practiced *today* (wall-clock minutes, whole minutes only).
    /// Reset to 0 when `today_date` rolls over.
    #[serde(default = "default_today_minutes_practiced")]
    pub today_minutes_practiced: u32,

    /// YYYY-MM-DD for the day the `today_minutes_practiced` counter refers to.
    /// Empty string means "no day yet" (e.g. fresh install or v1 migration).
    #[serde(default = "default_today_date")]
    pub today_date: String,
}

impl SavedStats {
    /// Return the stats file path, or `None` if the platform has no data dir.
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "keybr-tui").map(|dirs| dirs.data_dir().join("stats.json"))
    }

    /// Load stats from disk. Returns `None` if the file doesn't exist.
    /// Returns `None` with a warning if the file is malformed.
    pub fn load() -> Option<Self> {
        let path = Self::path()?;

        if !path.exists() {
            return None;
        }

        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<SavedStats>(&contents) {
                Ok(mut stats) => {
                    let today = today_date_string();
                    if stats.version < 2 {
                        // v1 → v2 migration: reset daily counter, bump version.
                        stats.today_minutes_practiced = 0;
                        stats.today_date = String::new();
                        stats.version = 2;
                    } else if stats.today_date != today {
                        // Same schema, but day rolled over while we were offline.
                        stats.today_minutes_practiced = 0;
                        stats.today_date = today;
                    }
                    Some(stats)
                }
                Err(e) => {
                    eprintln!(
                        "Warning: malformed stats at {}: {}. Starting fresh.",
                        path.display(),
                        e
                    );
                    None
                }
            },
            Err(e) => {
                eprintln!(
                    "Warning: could not read stats at {}: {}. Starting fresh.",
                    path.display(),
                    e
                );
                None
            }
        }
    }

    /// Save stats to disk, creating directories if needed.
    pub fn save(&self) -> color_eyre::Result<()> {
        let path = match Self::path() {
            Some(p) => p,
            None => return Ok(()),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    /// Delete the stats file from disk (for --reset).
    pub fn delete() -> color_eyre::Result<bool> {
        if let Some(path) = Self::path() {
            if path.exists() {
                std::fs::remove_file(&path)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_serialization_roundtrip() {
        let mut keys = HashMap::new();
        keys.insert(
            'e',
            SavedKeyStats {
                attempts: 100,
                errors: 5,
                filtered_time_ms: 350.0,
                best_filtered_time_ms: 280.0,
                recent_times_ms: vec![300, 310, 290, 320],
            },
        );
        keys.insert(
            't',
            SavedKeyStats {
                attempts: 80,
                errors: 3,
                filtered_time_ms: 400.0,
                best_filtered_time_ms: 350.0,
                recent_times_ms: vec![380, 390, 410],
            },
        );

        let stats = SavedStats {
            version: 2,
            keys,
            unlocked_letters: vec!['e', 't', 'a', 'o', 'i', 'n', 's'],
            total_lessons: 12,
            last_session: "2026-03-29T12:00:00Z".to_string(),
            today_minutes_practiced: 17,
            today_date: "2026-03-29".to_string(),
        };

        let json = serde_json::to_string_pretty(&stats).unwrap();
        let deserialized: SavedStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 2);
        assert_eq!(deserialized.total_lessons, 12);
        assert_eq!(deserialized.unlocked_letters.len(), 7);
        assert_eq!(deserialized.keys.len(), 2);
        assert_eq!(deserialized.today_minutes_practiced, 17);
        assert_eq!(deserialized.today_date, "2026-03-29");

        let e_stats = deserialized.keys.get(&'e').unwrap();
        assert_eq!(e_stats.attempts, 100);
        assert_eq!(e_stats.errors, 5);
        assert!((e_stats.filtered_time_ms - 350.0).abs() < f64::EPSILON);
        assert_eq!(e_stats.recent_times_ms.len(), 4);
    }

    #[test]
    fn stats_defaults_version() {
        let json = r#"{
            "keys": {},
            "unlocked_letters": [],
            "total_lessons": 0,
            "last_session": ""
        }"#;
        let stats: SavedStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.version, 1);
        // v1 JSON omits the daily-goal fields → serde fills defaults.
        assert_eq!(stats.today_minutes_practiced, 0);
        assert_eq!(stats.today_date, "");
    }

    #[test]
    fn date_string_from_known_unix_seconds() {
        // 2025-01-15 00:00:00 UTC → 1736899200
        assert_eq!(date_string_from_unix_secs(1_736_899_200), "2025-01-15");
        // Unix epoch
        assert_eq!(date_string_from_unix_secs(0), "1970-01-01");
        // 2000-02-29 (leap year) at 23:59:59 UTC → 951_868_799
        assert_eq!(date_string_from_unix_secs(951_868_799), "2000-02-29");
        // 2000-03-01 00:00:00 UTC → 951_868_800
        assert_eq!(date_string_from_unix_secs(951_868_800), "2000-03-01");
    }

    #[test]
    fn today_date_string_is_well_formed() {
        let s = today_date_string();
        assert_eq!(s.len(), 10, "expected YYYY-MM-DD, got {:?}", s);
        let bytes = s.as_bytes();
        assert_eq!(bytes[4], b'-');
        assert_eq!(bytes[7], b'-');
    }

    #[test]
    fn stats_path_is_some() {
        assert!(SavedStats::path().is_some());
    }
}
