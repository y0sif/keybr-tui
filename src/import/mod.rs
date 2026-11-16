//! Import a keybr.com data export (`typing-data.json`) and rebuild the
//! learning state exactly as keybr.com would compute it.
//!
//! keybr.com stores no derived state: on every page load it replays the
//! raw result list through a per-key EMA (alpha = 0.1 on each session's
//! mean `timeToType`) and derives unlocks/focus from the smoothed times.
//! We do the same replay once and persist the outcome as `SavedStats`,
//! so the user continues in the TUI right where the website left off.

mod replay;
mod report;
mod schema;
#[cfg(test)]
mod test_support;

use std::path::Path;

use color_eyre::eyre::{eyre, WrapErr};

use crate::engine::Language;
use crate::persistence::SavedStats;
use report::{family_report, nothing_imported};

pub use replay::{replay, ImportSummary};
pub use schema::{ExportRecord, LayoutChoice};

/// Full `--import` flow: parse the export, replay it, back up any existing
/// stats file, save, and print a report. Refuses to overwrite existing
/// stats unless `force` is set. `choice` picks the layout family
/// (`--layout`).
pub fn run_import(
    path: &Path,
    target_cpm: f64,
    force: bool,
    language: Language,
    choice: LayoutChoice,
) -> color_eyre::Result<()> {
    let stats_path = SavedStats::path(language)
        .ok_or_else(|| eyre!("could not determine the data directory"))?;
    if stats_path.exists() && !force {
        return Err(eyre!(
            "stats already exist at {}: re-run with --force to replace them \
             (a .bak backup will be kept)",
            stats_path.display()
        ));
    }

    let contents = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("could not read {}", path.display()))?;
    let records: Vec<ExportRecord> = serde_json::from_str(&contents)
        .wrap_err("not a keybr.com export: expected a JSON array of result records")?;

    let (saved, summary) = replay(&records, target_cpm, language, choice);
    if summary.imported == 0 {
        return Err(eyre!(nothing_imported(language, path, &summary)));
    }

    if stats_path.exists() {
        let backup = stats_path.with_extension("json.bak");
        std::fs::copy(&stats_path, &backup)
            .wrap_err_with(|| format!("could not back up stats to {}", backup.display()))?;
        println!("Backed up existing stats to {}", backup.display());
    }
    saved.save(language)?;

    println!(
        "Imported {} of {} sessions as {} ({} skipped: other layout, {} skipped: invalid).",
        summary.imported,
        summary.total,
        language.display_name(),
        summary.skipped_layout,
        summary.skipped_invalid
    );
    for line in family_report(&summary, choice) {
        println!("{line}");
    }
    println!(
        "Unlocked letters: {}/{}{}, derived against your {:.0} WPM target \
         (from --target-wpm or config.toml; changing it re-derives, like keybr.com).",
        summary.unlocked_letters,
        language.letters().len(),
        summary
            .focused_key
            .map(|c| format!(" (current focus: '{c}')"))
            .unwrap_or_default(),
        target_cpm / 5.0
    );
    println!(
        "Total practice time: {:.1} hours across {} sessions.",
        summary.total_practice_secs as f64 / 3600.0,
        summary.imported
    );
    if let Some(wpm) = summary.recent_wpm {
        println!(
            "Recent speed: ~{wpm:.0} WPM (last {} sessions).",
            10.min(summary.imported)
        );
    }
    println!("Saved to {}", stats_path.display());
    println!("Run `keybr-tui` to continue where you left off.");
    Ok(())
}
