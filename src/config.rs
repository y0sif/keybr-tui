use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn default_target_wpm() -> u32 {
    35
}

fn default_fragment_length() -> usize {
    100
}

/// Serializable error mode for config file.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorModeSerde {
    #[default]
    ForgiveMistakes,
    StopOnError,
}

/// User configuration, persisted to `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_target_wpm")]
    pub target_wpm: u32,

    #[serde(default)]
    pub error_mode: ErrorModeSerde,

    #[serde(default = "default_fragment_length")]
    pub fragment_length: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_wpm: default_target_wpm(),
            error_mode: ErrorModeSerde::default(),
            fragment_length: default_fragment_length(),
        }
    }
}

impl Config {
    /// Return the config file path, or `None` if the platform has no config dir.
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "keybr-tui")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load config from disk. Returns defaults if the file doesn't exist or is
    /// malformed (with a warning printed to stderr).
    pub fn load() -> Self {
        let path = match Self::path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!(
                        "Warning: malformed config at {}: {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!(
                    "Warning: could not read config at {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Save config to disk, creating directories if needed.
    pub fn save(&self) -> color_eyre::Result<()> {
        let path = match Self::path() {
            Some(p) => p,
            None => return Ok(()),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_serialization_roundtrip() {
        let cfg = Config {
            target_wpm: 50,
            error_mode: ErrorModeSerde::StopOnError,
            fragment_length: 120,
        };
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.target_wpm, 50);
        assert_eq!(deserialized.error_mode, ErrorModeSerde::StopOnError);
        assert_eq!(deserialized.fragment_length, 120);
    }

    #[test]
    fn config_defaults_on_empty_toml() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.target_wpm, 35);
        assert_eq!(cfg.error_mode, ErrorModeSerde::ForgiveMistakes);
        assert_eq!(cfg.fragment_length, 100);
    }

    #[test]
    fn config_ignores_unknown_keys() {
        let toml_str = r#"
            target_wpm = 40
            some_future_key = "hello"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.target_wpm, 40);
    }

    #[test]
    fn config_path_is_some() {
        // On most platforms this should succeed
        assert!(Config::path().is_some());
    }
}
