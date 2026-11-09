use std::collections::HashMap;
use std::path::{Path, PathBuf};

use color_eyre::eyre::WrapErr;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::engine::Language;
use crate::metrics::KeyStats;

use super::files::{read_user_file, write_atomic, Loaded};
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

    /// Load `language`'s stats for launch. `None` for no file, or for a
    /// broken one, which is moved aside first (see `load_from`) with a
    /// warning on stderr. Call it before the TUI starts, so the warning
    /// is still on screen after exit.
    pub fn load(language: Language) -> Option<Self> {
        Self::load_from(&Self::path(language)?).found_or_print()
    }

    /// Load the stats file at `path`, migrating older formats. A file that
    /// is there but cannot be read or parsed is moved aside to
    /// `<name>.corrupt-<unix seconds>` before this returns, so starting
    /// fresh never overwrites it.
    pub fn load_from(path: &Path) -> Loaded<Self> {
        read_user_file(path, "stats", "Starting fresh.", |contents| {
            serde_json::from_str::<SavedStats>(contents)
        })
        .map(SavedStats::migrated)
    }

    /// Bring stats read from an older file up to the current format and
    /// day.
    fn migrated(mut self) -> Self {
        let today = today_date_string();
        if self.version < 2 {
            // v1 → v2 migration: reset daily counter, bump version.
            self.today_seconds_practiced = 0;
            self.today_date = String::new();
            self.version = 2;
        } else {
            // v0.2.0 → v0.2.1 in-schema migration: minutes → seconds.
            // Pre-fix `today_minutes_practiced` was almost always 0
            // because of the integer-divide bug, but be defensive.
            if let Some(legacy_min) = self.today_minutes_practiced.take() {
                if self.today_seconds_practiced == 0 && legacy_min > 0 {
                    self.today_seconds_practiced = legacy_min.saturating_mul(60);
                }
            }
            if self.today_date != today {
                // Same schema, but day rolled over while we were offline.
                self.today_seconds_practiced = 0;
                self.today_date = today;
            }
        }
        self
    }

    /// The file contents a save writes.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Save `language`'s stats now, on this thread. For `--import`; the
    /// TUI queues its saves on `persistence::Saver` instead.
    pub fn save(&self, language: Language) -> color_eyre::Result<()> {
        match Self::path(language) {
            Some(path) => self.save_to(&path),
            None => Ok(()),
        }
    }

    /// Save to `path` atomically (see `write_atomic`), creating
    /// directories if needed.
    pub fn save_to(&self, path: &Path) -> color_eyre::Result<()> {
        write_atomic(path, self.to_json()?.as_bytes())
            .wrap_err_with(|| format!("could not save stats to {}", path.display()))
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
    use crate::persistence::test_support::TestDir;

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

    // --- On disk, in a throwaway directory (never the real data dir) ---

    fn sample_stats(total_lessons: u32) -> SavedStats {
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
        SavedStats {
            version: 2,
            keys,
            unlocked_letters: vec!['e', 't', 'a', 'o', 'i', 'n'],
            total_lessons,
            last_session: "1762700000".to_string(),
            today_seconds_practiced: 0,
            today_minutes_practiced: None,
            today_date: today_date_string(),
            last_lesson: None,
            lesson_history: Vec::new(),
        }
    }

    #[test]
    fn stats_save_to_and_load_from_round_trip() {
        let dir = TestDir::new("stats-round-trip");
        let path = dir.path().join("data").join("stats.json");
        sample_stats(12).save_to(&path).unwrap();
        let Loaded::Found(back) = SavedStats::load_from(&path) else {
            panic!("expected the saved stats back");
        };
        assert_eq!(back.total_lessons, 12);
        assert_eq!(back.keys[&'e'].attempts, 100);
        assert_eq!(dir.entries_in("data"), ["stats.json"]);
    }

    #[test]
    fn a_missing_stats_file_starts_fresh_silently() {
        let dir = TestDir::new("stats-missing");
        let loaded = SavedStats::load_from(&dir.path().join("stats.json"));
        assert!(matches!(loaded, Loaded::Missing));
        assert!(dir.entries().is_empty());
    }

    /// Load a stats file holding `broken`, then save fresh stats, checking
    /// that the broken bytes were kept aside and never overwritten.
    fn assert_broken_stats_are_kept(name: &str, broken: &[u8]) {
        let dir = TestDir::new(name);
        let path = dir.path().join("stats-de.json");
        std::fs::write(&path, broken).unwrap();

        let Loaded::Broken(warning) = SavedStats::load_from(&path) else {
            panic!("a broken file must not load");
        };
        let aside = dir.only_file_starting_with("stats-de.json.corrupt-");
        assert_eq!(std::fs::read(&aside).unwrap(), broken);
        assert!(!path.exists(), "the broken file must be moved, not copied");
        assert!(
            warning.starts_with("Warning: malformed stats at "),
            "{warning}"
        );
        assert!(
            warning.contains(&format!("Moved it to {}", aside.display())),
            "the warning must say where it went: {warning}"
        );
        assert!(warning.ends_with("Starting fresh."), "{warning}");

        // Starting fresh then saving writes a new file beside it.
        sample_stats(1).save_to(&path).unwrap();
        assert!(matches!(SavedStats::load_from(&path), Loaded::Found(s) if s.total_lessons == 1));
        assert_eq!(std::fs::read(&aside).unwrap(), broken);
        assert_eq!(dir.entries().len(), 2, "{:?}", dir.entries());
    }

    #[test]
    fn an_empty_stats_file_is_kept_aside_not_overwritten() {
        // What a save cut short by a closed terminal used to leave.
        assert_broken_stats_are_kept("stats-empty", b"");
    }

    #[test]
    fn a_truncated_stats_file_is_kept_aside_not_overwritten() {
        let json = sample_stats(41).to_json().unwrap();
        let half = &json.as_bytes()[..json.len() / 2];
        assert_broken_stats_are_kept("stats-truncated", half);
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
