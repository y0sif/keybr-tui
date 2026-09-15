use std::collections::HashMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

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

/// Compute YYYY-MM-DD for the current *local* calendar day.
///
/// The daily goal is a "did I practice today" counter, so it has to roll over
/// at local midnight, not UTC midnight — otherwise anyone west of UTC sees
/// evening practice counted toward tomorrow. The offset comes from libc's
/// `localtime_r` (honours `TZ`); non-unix targets fall back to UTC.
///
/// Uses Howard Hinnant's "civil_from_days" algorithm (public domain) to map
/// days-since-1970-01-01 → (year, month, day). Avoids pulling in chrono.
/// Reference: https://howardhinnant.github.io/date_algorithms.html
pub fn today_date_string() -> String {
    match unix_now_secs() {
        Some(now) => today_date_string_at(now),
        None => String::new(),
    }
}

/// Pure core of [`today_date_string`], evaluated at a caller-supplied instant.
///
/// This is the seam that lets a caller derive the local date *and* the local-day
/// UTC window from a single `SystemTime::now()` reading. Reading the clock twice
/// can straddle local midnight and label day D's seconds with day D+1's date.
pub fn today_date_string_at(now: i64) -> String {
    date_string_from_unix_secs(now + local_utc_offset_secs(now))
}

/// Seconds since the Unix epoch, or `None` if the clock is set before it.
///
/// The single place the wall clock is read for day accounting, so callers that
/// need several clock-derived values can take one reading and pass it down.
pub fn unix_now_secs() -> Option<i64> {
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => Some(d.as_secs() as i64),
        Err(_) => None,
    }
}

/// Seconds east of UTC for the local timezone at the given Unix time.
#[cfg(unix)]
fn local_utc_offset_secs(secs: i64) -> i64 {
    let t: libc::time_t = secs as libc::time_t;
    // SAFETY: `tm` is a plain C struct; localtime_r only writes into the
    // out-pointer we give it and is thread-safe.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i64
}

#[cfg(not(unix))]
fn local_utc_offset_secs(_secs: i64) -> i64 {
    0
}

/// Pure helper: (year, month, day) for a count of days since 1970-01-01.
///
/// Howard Hinnant's "civil_from_days" (public domain).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift epoch from 1970-01-01 → 0000-03-01 (Hinnant's "civil_from_days").
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe: u32 = (z - era * 146_097) as u32; // [0, 146096]
    let yoe: u32 = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y: i64 = yoe as i64 + era * 400;
    let doy: u32 = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp: u32 = (5 * doy + 2) / 153; // [0, 11]
    let d: u32 = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m: u32 = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year: i64 = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Pure helper: format the UTC date for a Unix timestamp in seconds as YYYY-MM-DD.
fn date_string_from_unix_secs(secs: i64) -> String {
    // Days since 1970-01-01 (UTC). Floor division so negatives round down.
    let (year, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{:04}-{:02}-{:02}", year, m, d)
}

/// Pure helper: format a Unix timestamp as fixed-width `YYYY-MM-DDTHH:MM:SS` (UTC).
///
/// Fixed width matters: the keybr.com export stamps every record in UTC with
/// the same layout, so string comparison against these bounds is the same
/// ordering as comparing the instants.
pub fn datetime_string_from_unix_secs(secs: i64) -> String {
    let (year, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400); // seconds within the day, always >= 0
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year,
        m,
        d,
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

/// UTC bounds of the *local* calendar day containing `now`, as the half-open
/// interval `[start, end)` formatted `YYYY-MM-DDTHH:MM:SS`.
///
/// Anything that has to decide "did this UTC-stamped event happen today?"
/// must compare against this window rather than string-matching a local date
/// prefix: the local day is a 24h window offset from UTC, so its UTC bounds
/// generally land mid-day on two different UTC dates.
///
/// Takes the instant rather than reading the clock so that a caller needing
/// both this window and [`today_date_string_at`] can derive them from one
/// reading — see [`unix_now_secs`].
///
/// Known limitation, DST transition days: `end` is `start + 86_400` with the
/// UTC offset sampled at `now` used for *both* edges, so on the two days a year
/// a DST-observing zone shifts, the real local day is 23h or 25h long and one
/// edge is off by the DST delta. Concretely: a user in America/New_York
/// importing at 11:00 local on 2025-11-02 gets a window starting 05:00Z while
/// the true local day started 04:00Z, so a record at 04:30Z (00:30 EDT, the
/// same local day) is excluded. A 15-minute sweep of all of 2025 found exactly
/// 2 wrong local days per year in DST-observing zones (America/New_York,
/// Europe/London, Europe/Berlin; ±1800s on Australia/Lord_Howe) and 0 in
/// Asia/Kolkata, Asia/Tokyo and UTC — while the invariant the import actually
/// depends on, that the window always contains `now`, held at all 35,040
/// sampled instants. This affects `--import` only, never live rollover: the
/// running app uses [`today_date_string`], which samples the offset at the
/// instant it formats and so is always correct for "now".
pub fn local_day_utc_bounds_at(now: i64) -> (String, String) {
    let offset = local_utc_offset_secs(now);
    let local_secs = now + offset;
    // div_euclid, not `/`: pre-1970 (or far-west offsets near the epoch) must
    // floor toward the earlier day, not truncate toward zero.
    let day_start_local = local_secs.div_euclid(86_400) * 86_400;
    let start_utc = day_start_local - offset;
    let end_utc = start_utc + 86_400;
    (
        datetime_string_from_unix_secs(start_utc),
        datetime_string_from_unix_secs(end_utc),
    )
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

    // libc exposes `tzset` only on Windows, so declare it ourselves for unix.
    #[cfg(unix)]
    extern "C" {
        fn tzset();
    }

    /// Serializes every TZ-mutating test against every other one.
    ///
    /// This bounds, but does not eliminate, the glibc `setenv`/`getenv` data
    /// race: unrelated tests running on other threads may still read the
    /// environment while we are writing it. (That hazard is exactly why
    /// edition 2024 made `std::env::set_var` `unsafe`.) Nothing short of
    /// running these in their own process removes it; the lock at least keeps
    /// the TZ tests from clobbering each other.
    #[cfg(unix)]
    static TZ_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard: sets `TZ` (+ `tzset()`) for its lifetime and restores the
    /// previous value on drop — including the "was not set at all" case, which
    /// restores by removing the variable. `Drop` runs during unwinding, so a
    /// panicking test still puts the environment back; no `catch_unwind`
    /// needed. The mutex guard is held for the same lifetime, and a poisoned
    /// lock is recovered so one failing test doesn't cascade into the rest.
    ///
    /// Do not nest: `TZ_LOCK` is a plain, non-reentrant `Mutex`, so calling
    /// `TzGuard::set` inside a live guard's scope deadlocks silently.
    #[cfg(unix)]
    struct TzGuard {
        prev: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    #[cfg(unix)]
    impl TzGuard {
        fn set(tz: &str) -> Self {
            Self::with_lock(lock_tz(), tz)
        }

        /// Takes an already-held lock, for the one test that has to read `TZ`
        /// and then swap it without the two steps being interleaved.
        fn with_lock(lock: std::sync::MutexGuard<'static, ()>, tz: &str) -> Self {
            let prev = std::env::var_os("TZ");
            std::env::set_var("TZ", tz);
            // SAFETY: plain libc call with no arguments; serialized by TZ_LOCK.
            unsafe { tzset() };
            Self { prev, _lock: lock }
        }
    }

    /// A poisoned lock is recovered rather than propagated: one panicking TZ
    /// test must not cascade into every later one.
    #[cfg(unix)]
    fn lock_tz() -> std::sync::MutexGuard<'static, ()> {
        TZ_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(unix)]
    impl Drop for TzGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var("TZ", v),
                None => std::env::remove_var("TZ"),
            }
            // SAFETY: as above.
            unsafe { tzset() };
        }
    }

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

    /// The load-bearing test for the local-timezone fix.
    ///
    /// At *any* instant the local calendar date at UTC+14 and at UTC-12
    /// differ, because the two zones are 26 hours apart and a day is only 24 —
    /// so this needs no pinned wall clock to be deterministic. Drop the
    /// `+ local_utc_offset_secs(secs)` term from `today_date_string()` and both
    /// calls collapse to the same UTC date, failing the assert.
    ///
    /// POSIX `TZ` offsets are sign-inverted relative to ISO: `UTC-14` is 14h
    /// EAST of UTC (+1400) and `UTC+12` is 12h WEST (-1200). POSIX offset
    /// strings, not zone names, so no tzdata install is required.
    #[cfg(unix)]
    #[test]
    fn today_date_string_uses_local_timezone() {
        let east = {
            let _tz = TzGuard::set("UTC-14");
            today_date_string()
        };
        let west = {
            let _tz = TzGuard::set("UTC+12");
            today_date_string()
        };
        assert_ne!(
            east, west,
            "today_date_string() returned {east} at UTC+14 and {west} at UTC-12 — \
             zones 26h apart can never share a calendar date, so the local offset \
             is being ignored"
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_offset_and_date_for_pinned_instant() {
        // Deterministic value check to complement the not-equal test above.
        // 2025-01-15 23:00 UTC is still Jan 15 in UTC but Jan 16 at UTC+6.
        let _tz = TzGuard::set("UTC-6"); // POSIX: 6h east → +0600
        assert_eq!(local_utc_offset_secs(1_736_982_000), 6 * 3600);
        assert_eq!(
            date_string_from_unix_secs(1_736_982_000 + local_utc_offset_secs(1_736_982_000)),
            "2025-01-16"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tz_guard_restores_previous_value() {
        // Snapshot under the lock and hand that same lock to the guard: read
        // it unlocked and a concurrently-running TZ test's guard would be the
        // value we snapshot, so the restore check would compare against a
        // value that was never really ours.
        let lock = lock_tz();
        let before = std::env::var_os("TZ");
        {
            let _tz = TzGuard::with_lock(lock, "UTC-9");
            assert_eq!(std::env::var("TZ").ok().as_deref(), Some("UTC-9"));
        }
        // Re-take the lock before reading back: that guarantees no other
        // guard is mid-swap, so what we see is the settled value.
        let _lock = lock_tz();
        assert_eq!(std::env::var_os("TZ"), before);
    }

    #[test]
    fn datetime_string_is_fixed_width_utc() {
        assert_eq!(
            datetime_string_from_unix_secs(1_736_899_200),
            "2025-01-15T00:00:00"
        );
        assert_eq!(
            datetime_string_from_unix_secs(1_736_899_200 + 73_800),
            "2025-01-15T20:30:00"
        );
        assert_eq!(datetime_string_from_unix_secs(0), "1970-01-01T00:00:00");
        // Pre-epoch: rem_euclid must wrap into the previous day, not go negative.
        assert_eq!(datetime_string_from_unix_secs(-1), "1969-12-31T23:59:59");
        assert_eq!(datetime_string_from_unix_secs(1_736_899_200).len(), 19);
    }

    #[cfg(unix)]
    #[test]
    fn local_day_bounds_east_of_utc() {
        // Local zone UTC+6 (POSIX spells it "UTC-6").
        // 2025-01-15 20:30 UTC = 2025-01-16 02:30 local, so "today" locally is
        // Jan 16, whose UTC window is [Jan 15 18:00, Jan 16 18:00).
        let _tz = TzGuard::set("UTC-6");
        let (start, end) = local_day_utc_bounds_at(1_736_973_000);
        assert_eq!(start, "2025-01-15T18:00:00");
        assert_eq!(end, "2025-01-16T18:00:00");
    }

    #[cfg(unix)]
    #[test]
    fn local_day_bounds_west_of_utc() {
        // Local zone UTC-6 (POSIX spells it "UTC+6").
        // 2025-01-16 01:00 UTC = 2025-01-15 19:00 local, so "today" locally is
        // Jan 15, whose UTC window is [Jan 15 06:00, Jan 16 06:00).
        let _tz = TzGuard::set("UTC+6");
        let (start, end) = local_day_utc_bounds_at(1_736_989_200);
        assert_eq!(start, "2025-01-15T06:00:00");
        assert_eq!(end, "2025-01-16T06:00:00");
    }

    #[cfg(unix)]
    #[test]
    fn local_day_bounds_always_contain_the_instant_they_were_built_from() {
        // Sweep a whole UTC day at 37-minute steps in the two extreme zones:
        // the window must contain `now` at every step, which is the property
        // the import range check depends on.
        for tz in ["UTC-14", "UTC+12"] {
            let _tz = TzGuard::set(tz);
            let mut now = 1_736_899_200; // 2025-01-15T00:00:00Z
            while now < 1_736_899_200 + 86_400 {
                let (start, end) = local_day_utc_bounds_at(now);
                let now_s = datetime_string_from_unix_secs(now);
                assert!(
                    now_s >= start && now_s < end,
                    "{tz}: {now_s} outside [{start}, {end})"
                );
                now += 37 * 60;
            }
        }
    }

    #[test]
    fn local_day_bounds_live_clock_is_well_formed() {
        let now = unix_now_secs().expect("system clock before the epoch");
        let (start, end) = local_day_utc_bounds_at(now);
        assert_eq!(start.len(), 19);
        assert_eq!(end.len(), 19);
        assert!(start < end);
    }

    #[test]
    fn stats_path_is_some() {
        assert!(SavedStats::path().is_some());
    }
}
