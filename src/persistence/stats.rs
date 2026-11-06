use std::collections::HashMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::engine::Language;
use crate::metrics::KeyStats;

use super::localtime::today_date_string;

fn default_version() -> u32 {
    1
}

fn default_today_seconds_practiced() -> u32 {
    0
}

fn default_today_date() -> String {
    String::new()
}

fn default_legacy_minutes() -> Option<u32> {
    None
}

fn default_last_lesson() -> Option<SavedLessonResult> {
    None
}

fn default_lesson_history() -> Vec<SavedLessonResult> {
    Vec::new()
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

/// Moves the recent times across, so a caller that owns the stats pays
/// for no copy.
impl From<KeyStats> for SavedKeyStats {
    fn from(st: KeyStats) -> Self {
        SavedKeyStats {
            attempts: st.attempts,
            errors: st.errors,
            filtered_time_ms: st.filtered_time_ms,
            best_filtered_time_ms: st.best_filtered_time_ms,
            recent_times_ms: st.reaction_times_ms,
        }
    }
}

impl From<&KeyStats> for SavedKeyStats {
    fn from(st: &KeyStats) -> Self {
        SavedKeyStats::from(st.clone())
    }
}

impl From<&SavedKeyStats> for KeyStats {
    fn from(saved: &SavedKeyStats) -> Self {
        let mut st = KeyStats::default();
        st.attempts = saved.attempts;
        st.errors = saved.errors;
        st.filtered_time_ms = saved.filtered_time_ms;
        st.best_filtered_time_ms = saved.best_filtered_time_ms;
        // Restore recent times (the most recent, up to KeyStats' cap)
        let recent = &saved.recent_times_ms;
        let start = recent.len().saturating_sub(KeyStats::RECENT_CAP);
        st.reaction_times_ms = recent[start..].to_vec();
        st
    }
}

/// Persisted form of a completed lesson's headline stats. The
/// `newly_unlocked` field is intentionally not stored — the "+letter
/// unlocked!" callout is meant to celebrate the event in the moment,
/// not re-show on every app launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLessonResult {
    pub wpm: f64,
    pub accuracy: f64,
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

    /// Seconds practiced *today* (wall-clock seconds).
    /// Reset to 0 when `today_date` rolls over. Stored in seconds to avoid
    /// floor-to-zero on sub-minute lessons.
    #[serde(default = "default_today_seconds_practiced")]
    pub today_seconds_practiced: u32,

    /// Pre-v0.2.1 minutes field. Read-only on load (for one-time migration).
    /// Always written as `None` so it disappears from new files.
    #[serde(default = "default_legacy_minutes", skip_serializing)]
    pub today_minutes_practiced: Option<u32>,

    /// YYYY-MM-DD for the day the `today_seconds_practiced` counter refers to.
    /// Empty string means "no day yet" (e.g. fresh install or v1 migration).
    #[serde(default = "default_today_date")]
    pub today_date: String,

    /// Most recent completed lesson's headline stats. The dashboard's
    /// Metrics row reads from this; persisting it means a fresh launch
    /// shows the previous session's numbers instead of "—" placeholders.
    #[serde(default = "default_last_lesson")]
    pub last_lesson: Option<SavedLessonResult>,

    /// Rolling history used to compute deltas (vs running mean) in the
    /// dashboard. Kept short — the engine itself caps this at 50 entries.
    #[serde(default = "default_lesson_history")]
    pub lesson_history: Vec<SavedLessonResult>,
}

impl SavedStats {
    /// Return the stats file path for `language`, or `None` if the platform
    /// has no data dir. Each language has its own file (per-key stats for
    /// `i` in English and Turkish are different keys on different
    /// keyboards), and English keeps the original `stats.json`.
    pub fn path(language: Language) -> Option<PathBuf> {
        ProjectDirs::from("", "", "keybr-tui")
            .map(|dirs| dirs.data_dir().join(language.stats_file_name()))
    }

    /// Load stats from disk. Returns `None` if the file doesn't exist.
    /// Returns `None` with a warning if the file is malformed.
    pub fn load(language: Language) -> Option<Self> {
        let path = Self::path(language)?;

        if !path.exists() {
            return None;
        }

        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<SavedStats>(&contents) {
                Ok(mut stats) => {
                    let today = today_date_string();
                    if stats.version < 2 {
                        // v1 → v2 migration: reset daily counter, bump version.
                        stats.today_seconds_practiced = 0;
                        stats.today_date = String::new();
                        stats.version = 2;
                    } else {
                        // v0.2.0 → v0.2.1 in-schema migration: minutes → seconds.
                        // Pre-fix `today_minutes_practiced` was almost always 0
                        // because of the integer-divide bug, but be defensive.
                        if let Some(legacy_min) = stats.today_minutes_practiced.take() {
                            if stats.today_seconds_practiced == 0 && legacy_min > 0 {
                                stats.today_seconds_practiced = legacy_min.saturating_mul(60);
                            }
                        }
                        if stats.today_date != today {
                            // Same schema, but day rolled over while we were offline.
                            stats.today_seconds_practiced = 0;
                            stats.today_date = today;
                        }
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
    pub fn save(&self, language: Language) -> color_eyre::Result<()> {
        let path = match Self::path(language) {
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
    pub fn delete(language: Language) -> color_eyre::Result<bool> {
        if let Some(path) = Self::path(language) {
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
            today_seconds_practiced: 1042,
            today_minutes_practiced: None,
            today_date: "2026-03-29".to_string(),
            last_lesson: Some(SavedLessonResult {
                wpm: 42.5,
                accuracy: 96.0,
            }),
            lesson_history: vec![
                SavedLessonResult {
                    wpm: 38.0,
                    accuracy: 94.0,
                },
                SavedLessonResult {
                    wpm: 42.5,
                    accuracy: 96.0,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&stats).unwrap();
        let deserialized: SavedStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 2);
        assert_eq!(deserialized.total_lessons, 12);
        assert_eq!(deserialized.unlocked_letters.len(), 7);
        assert_eq!(deserialized.keys.len(), 2);
        assert_eq!(deserialized.today_seconds_practiced, 1042);
        assert_eq!(deserialized.today_date, "2026-03-29");
        let last = deserialized.last_lesson.as_ref().unwrap();
        assert!((last.wpm - 42.5).abs() < f64::EPSILON);
        assert!((last.accuracy - 96.0).abs() < f64::EPSILON);
        assert_eq!(deserialized.lesson_history.len(), 2);

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
        // v1 JSON omits the daily-goal + lesson fields → serde fills defaults.
        assert_eq!(stats.today_seconds_practiced, 0);
        assert_eq!(stats.today_minutes_practiced, None);
        assert_eq!(stats.today_date, "");
        assert!(stats.last_lesson.is_none());
        assert!(stats.lesson_history.is_empty());
    }

    #[test]
    fn legacy_minutes_field_deserializes() {
        // Old v2 file written by v0.2.0 has today_minutes_practiced (no seconds).
        // Should land in the legacy Option so the load-time migration can pick it up.
        let json = r#"{
            "version": 2,
            "keys": {},
            "unlocked_letters": [],
            "total_lessons": 0,
            "last_session": "",
            "today_minutes_practiced": 7,
            "today_date": "2026-03-29"
        }"#;
        let stats: SavedStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.today_minutes_practiced, Some(7));
        assert_eq!(stats.today_seconds_practiced, 0);
    }

    #[test]
    fn stats_path_is_some() {
        assert!(SavedStats::path(Language::English).is_some());
        let en = SavedStats::path(Language::English).unwrap();
        let tr = SavedStats::path(Language::Turkish).unwrap();
        assert_eq!(en.file_name().unwrap(), "stats.json");
        assert_eq!(tr.file_name().unwrap(), "stats-tr.json");
        assert_eq!(en.parent(), tr.parent());
    }
}
