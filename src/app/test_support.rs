//! Test helpers shared by the app tests.

use super::App;

/// Unlock the whole alphabet, which is what makes every combination
/// drill reachable. Mirrors what the scheduler does over a long
/// profile, without having to type thousands of characters.
///
/// Writing `active_keys` by hand bypasses `run_scheduler_update`, so
/// the derived drill list has to be refreshed the same way the
/// scheduler path does it.
pub(super) fn unlock_all_letters(app: &mut App) {
    set_active_keys(app, ('a'..='z').collect());
}

/// Replace the unlocked alphabet and refresh what is derived from it.
/// The only way these tests should write `active_keys`.
pub(super) fn set_active_keys(app: &mut App, keys: Vec<char>) {
    app.scheduler.active_keys = keys;
    app.refresh_available_patterns();
}
