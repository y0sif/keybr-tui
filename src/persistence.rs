use std::collections::HashMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
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
                Ok(stats) => Some(stats),
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
            version: 1,
            keys,
            unlocked_letters: vec!['e', 't', 'a', 'o', 'i', 'n', 's'],
            total_lessons: 12,
            last_session: "2026-03-29T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string_pretty(&stats).unwrap();
        let deserialized: SavedStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.total_lessons, 12);
        assert_eq!(deserialized.unlocked_letters.len(), 7);
        assert_eq!(deserialized.keys.len(), 2);

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
    }

    #[test]
    fn stats_path_is_some() {
        assert!(SavedStats::path().is_some());
    }
}
