use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn default_target_wpm() -> u32 {
    35
}

fn default_fragment_length() -> usize {
    100
}

fn default_natural_words() -> bool {
    true
}

fn default_daily_goal_minutes() -> u32 {
    30
}

fn default_alphabet_size() -> f64 {
    0.0
}

/// Deserialize `alphabet_size`, clamping out-of-range values to [0.0, 1.0]
/// (non-finite values fall back to the default of 0.0).
fn de_alphabet_size<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    if value.is_finite() {
        Ok(value.clamp(0.0, 1.0))
    } else {
        Ok(default_alphabet_size())
    }
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

    /// When true, the word generator prefers real English dictionary
    /// words; when none match the active letter filter, it falls back
    /// to the phonetic order-4 model.
    #[serde(default = "default_natural_words")]
    pub natural_words: bool,

    /// Daily practice goal in minutes. 0 hides the daily-goal indicator
    /// in the dashboard. Defaults to 30 to match the in-app default.
    #[serde(default = "default_daily_goal_minutes")]
    pub daily_goal_minutes: u32,

    /// Fraction of the non-starter alphabet to force-include regardless of
    /// confidence (keybr's `alphabetSize`). Clamped to [0.0, 1.0] on load;
    /// 0.0 (default) keeps the pure earn-by-confidence progression.
    #[serde(
        default = "default_alphabet_size",
        deserialize_with = "de_alphabet_size"
    )]
    pub alphabet_size: f64,

    /// User-pinned focus letter (keybr's manual lesson focus); `None`
    /// means the scheduler picks automatically. The `skip_serializing_if`
    /// is load-bearing: TOML cannot represent `None`, so serializing it
    /// would make `Config::save()` error — the key is omitted instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_letter: Option<char>,

    /// User-pinned combination drill: a bigram (`"cr"`) or a word ending
    /// (`"-tion"`), in `FocusRule::config_value` spelling. `None` means no
    /// pattern pin. A usable value here outranks `focus_letter`; the two
    /// resolve to one runtime pin in `App::set_manual_focus_from_config`.
    ///
    /// Deliberately a second key rather than a widened `focus_letter`.
    /// `focus_letter` is a `char`, so an older keybr-tui reading `"cr"`
    /// there would fail to parse the file, and `load()` answers a parse
    /// failure by discarding the *whole* config: target WPM, error mode,
    /// fragment length and alphabet size would all silently reset. An
    /// unknown key is ignored instead, which costs the user nothing but
    /// the pin. `skip_serializing_if` for the same reason as
    /// `focus_letter`: TOML cannot represent `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_pattern: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_wpm: default_target_wpm(),
            error_mode: ErrorModeSerde::default(),
            fragment_length: default_fragment_length(),
            natural_words: default_natural_words(),
            daily_goal_minutes: default_daily_goal_minutes(),
            alphabet_size: default_alphabet_size(),
            focus_letter: None,
            focus_pattern: None,
        }
    }
}

impl Config {
    /// Return the config file path, or `None` if the platform has no config dir.
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "keybr-tui").map(|dirs| dirs.config_dir().join("config.toml"))
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
            natural_words: false,
            daily_goal_minutes: 45,
            alphabet_size: 0.35,
            focus_letter: Some('c'),
            focus_pattern: None,
        };
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.target_wpm, 50);
        assert_eq!(deserialized.error_mode, ErrorModeSerde::StopOnError);
        assert_eq!(deserialized.fragment_length, 120);
        assert!(!deserialized.natural_words);
        assert_eq!(deserialized.daily_goal_minutes, 45);
        assert_eq!(deserialized.alphabet_size, 0.35);
        assert_eq!(deserialized.focus_letter, Some('c'));
        assert_eq!(deserialized.focus_pattern, None);
    }

    #[test]
    fn config_focus_pattern_roundtrips() {
        let cfg = Config {
            focus_pattern: Some("-tion".to_string()),
            ..Config::default()
        };
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.focus_pattern, Some("-tion".to_string()));
        assert_eq!(deserialized.focus_letter, None);
    }

    #[test]
    fn config_focus_pattern_none_is_omitted() {
        // Same contract as `focus_letter`: TOML has no `None`, so the key
        // must be omitted rather than serialized.
        let cfg = Config::default();
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        assert!(!serialized.contains("focus_pattern"));
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.focus_pattern, None);
    }

    #[test]
    fn config_with_focus_pattern_keeps_every_other_field() {
        // A file this build wrote: `focus_pattern` set alongside every
        // other setting. All of them must come back as written rather
        // than falling back to defaults. The back-compat half of the
        // story, an older build meeting a key it does not know, is
        // `unknown_key_is_ignored_instead_of_failing_the_load` below.
        let toml_str = r#"
            target_wpm = 65
            error_mode = "stop-on-error"
            fragment_length = 140
            natural_words = false
            daily_goal_minutes = 15
            alphabet_size = 0.4
            focus_pattern = "cr"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("focus_pattern must parse");
        assert_eq!(cfg.focus_pattern, Some("cr".to_string()));
        // Everything else survived rather than falling back to defaults.
        assert_eq!(cfg.target_wpm, 65);
        assert_eq!(cfg.error_mode, ErrorModeSerde::StopOnError);
        assert_eq!(cfg.fragment_length, 140);
        assert!(!cfg.natural_words);
        assert_eq!(cfg.daily_goal_minutes, 15);
        assert_eq!(cfg.alphabet_size, 0.4);
    }

    #[test]
    fn config_with_only_focus_letter_still_loads() {
        // A config file written by a pre-pattern build.
        let toml_str = r#"
            target_wpm = 45
            focus_letter = "r"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.target_wpm, 45);
        assert_eq!(cfg.focus_letter, Some('r'));
        assert_eq!(cfg.focus_pattern, None);
    }

    #[test]
    fn unknown_key_is_ignored_instead_of_failing_the_load() {
        // The guarantee that made `focus_pattern` a separate key rather
        // than a wider `focus_letter`: an older keybr-tui reading a
        // config this build wrote must ignore the key it does not know.
        // `load()` answers a parse error by throwing the whole file away,
        // so a rejected key would silently reset target WPM, error mode,
        // fragment length and alphabet size too.
        //
        // `Config` carries no `deny_unknown_fields`, which is what makes
        // that true, so this tests it directly with a key no build knows
        // rather than with `focus_pattern`, which this build does know.
        let toml_str = r#"
            target_wpm = 65
            error_mode = "stop-on-error"
            fragment_length = 140
            natural_words = false
            daily_goal_minutes = 15
            alphabet_size = 0.4
            focus_letter = "r"
            focus_from_the_future = "whatever this turns out to be"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("an unknown key must not fail the load");
        assert_eq!(cfg.target_wpm, 65);
        assert_eq!(cfg.error_mode, ErrorModeSerde::StopOnError);
        assert_eq!(cfg.fragment_length, 140);
        assert!(!cfg.natural_words);
        assert_eq!(cfg.daily_goal_minutes, 15);
        assert_eq!(cfg.alphabet_size, 0.4);
        assert_eq!(cfg.focus_letter, Some('r'));
    }

    #[test]
    fn unrecognised_focus_pattern_value_is_a_dropped_pin() {
        // An unknown *value* under a known key, which is what a retired
        // preset or a hand-edited typo looks like. Parsing must still
        // succeed and leave the rest of the config alone; the pin is
        // dropped later, when `FocusRule::from_config` fails to resolve
        // it, not by rejecting the file.
        let toml_str = r#"
            target_wpm = 65
            focus_pattern = "banana"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.target_wpm, 65);
        assert_eq!(cfg.focus_pattern, Some("banana".to_string()));
        assert_eq!(
            crate::engine::filter::FocusRule::from_config(cfg.focus_pattern.as_deref().unwrap()),
            None,
            "an unrecognised value must resolve to no pin",
        );
    }

    #[test]
    fn config_focus_letter_none_roundtrip() {
        // TOML has no `None`; serialization must omit the key rather than
        // error, and the omitted key must load back as `None`.
        let cfg = Config::default();
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        assert!(!serialized.contains("focus_letter"));
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.focus_letter, None);
    }

    #[test]
    fn config_without_focus_letter_loads_as_none() {
        let cfg: Config = toml::from_str("target_wpm = 40").unwrap();
        assert_eq!(cfg.focus_letter, None);
    }

    #[test]
    fn config_defaults_on_empty_toml() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.target_wpm, 35);
        assert_eq!(cfg.error_mode, ErrorModeSerde::ForgiveMistakes);
        assert_eq!(cfg.fragment_length, 100);
        assert!(cfg.natural_words);
        assert_eq!(cfg.daily_goal_minutes, 30);
        assert_eq!(cfg.alphabet_size, 0.0);
    }

    #[test]
    fn config_clamps_alphabet_size() {
        let cfg: Config = toml::from_str("alphabet_size = 1.5").unwrap();
        assert_eq!(cfg.alphabet_size, 1.0);
        let cfg: Config = toml::from_str("alphabet_size = -0.2").unwrap();
        assert_eq!(cfg.alphabet_size, 0.0);
        let cfg: Config = toml::from_str("alphabet_size = 0.5").unwrap();
        assert_eq!(cfg.alphabet_size, 0.5);
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
