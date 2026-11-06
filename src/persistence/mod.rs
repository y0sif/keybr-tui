//! What keybr-tui keeps between runs: the saved stats file (`stats`) and the
//! local-calendar-day helpers that decide when the daily counter rolls over
//! (`localtime`).

mod localtime;
mod stats;

// Only `import`'s tests format UTC datetimes from outside this module.
#[cfg(test)]
pub use localtime::datetime_string_from_unix_secs;
pub use localtime::{
    local_day_utc_bounds_at, today_date_string, today_date_string_at, unix_now_secs,
};
pub use stats::{SavedKeyStats, SavedLessonResult, SavedStats};
