//! What keybr-tui keeps between runs: the saved stats file (`stats`), the
//! local-calendar-day helpers that decide when the daily counter rolls over
//! (`localtime`), how user files are written and read (`files`: atomic
//! replace, and moving a broken file aside instead of overwriting it), and
//! the background writer every in-app save goes through (`saver`).

mod files;
mod localtime;
mod saver;
mod stats;
#[cfg(test)]
pub(crate) mod test_support;

pub use files::{read_user_file, write_atomic, Loaded};
// Only `import`'s tests format UTC datetimes from outside this module.
#[cfg(test)]
pub use localtime::datetime_string_from_unix_secs;
pub use localtime::{
    local_day_utc_bounds_at, today_date_string, today_date_string_at, unix_now_secs,
};
pub use saver::{SaveReport, Saver};
pub use stats::{SavedKeyStats, SavedLessonResult, SavedStats};
