mod app;
mod components;
mod config;
mod engine;
mod events;
mod import;
mod metrics;
mod persistence;
// The whole taria integration is unix-only: taria's transport is a unix
// domain socket, so `taria-ratatui` does not compile for Windows. Every
// touch point below is gated the same way, and the Windows build runs
// exactly as it did before taria existed.
#[cfg(unix)]
mod tree;
mod tui;
mod ui;
mod update;

use clap::Parser;
#[cfg(unix)]
use taria_ratatui::{InputStatus, TariaLayer};

use app::{App, ErrorMode};
use config::{Config, ErrorModeSerde};
use engine::Language;
use events::setup_event_channel;
use persistence::SavedStats;
use ui::view;
use update::update;
#[cfg(unix)]
use update::{apply_agent_input, Applied};

/// A terminal typing trainer inspired by keybr.com with adaptive learning.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Target typing speed in words per minute.
    #[arg(long)]
    target_wpm: Option<u32>,

    /// Error handling mode: "move-on" or "stop-on-error".
    #[arg(long)]
    error_mode: Option<String>,

    /// Delete saved stats and start fresh (keeps config).
    #[arg(long)]
    reset: bool,

    /// Print the data directory path and exit.
    #[arg(long)]
    data_dir: bool,

    /// Import a keybr.com data export (typing-data.json) and exit.
    /// Rebuilds per-key stats and unlocked letters from the full history.
    #[arg(long, value_name = "FILE")]
    import: Option<std::path::PathBuf>,

    /// With --import: replace existing stats (a .bak backup is written).
    #[arg(long)]
    force: bool,

    // Practice language; the help text lists the codes (`language_help`).
    #[arg(long, value_name = "CODE", value_parser = parse_language, help = language_help())]
    language: Option<Language>,
}

/// Help for `--language`, listing the codes from the language table so it
/// never goes stale when a language is added.
fn language_help() -> String {
    format!(
        "Practice language: {} (default en). Saved to config.toml, so later runs keep it. Each language keeps its own stats.",
        Language::supported_codes()
    )
}

/// Clap parser for `--language`: an unknown code is rejected on the spot,
/// with the usual clap error, since the user is right there to fix it.
fn parse_language(code: &str) -> Result<Language, String> {
    Language::from_code(code).ok_or_else(|| {
        format!(
            "unknown language (supported: {})",
            Language::supported_codes()
        )
    })
}

/// Resolve the practice language: `--language` wins over the config file.
/// An unknown code in the config file is only a warning, so a typo there
/// never stops the app from starting.
///
/// Saves nothing: `--data-dir` and `--import` use the language too, and a
/// failed import must not switch the config to it. `main` writes it back
/// (see `remember_language`) only after a successful import or once it is
/// about to launch the TUI.
fn resolve_language(cli: Option<Language>, config: &Config) -> Language {
    if let Some(language) = cli {
        return language;
    }
    Language::from_code(&config.language).unwrap_or_else(|| {
        eprintln!(
            "Warning: unknown language '{}' in config.toml. Using English.",
            config.language
        );
        Language::English
    })
}

/// The language code to write back to config.toml so that
/// `keybr-tui --language tr` once is enough: `--language`'s code, unless
/// the config already holds it (or the flag is absent).
fn language_to_save(cli: Option<Language>, config: &Config) -> Option<&'static str> {
    cli.map(Language::code)
        .filter(|&code| config.language != code)
}

/// Write `--language` back to config.toml (see `language_to_save`).
fn remember_language(cli: Option<Language>, config: &mut Config) {
    if let Some(code) = language_to_save(cli, config) {
        config.language = code.to_string();
        if let Err(e) = config.save() {
            eprintln!("Warning: failed to save language to config: {e}");
        }
    }
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    let mut config = Config::load();
    let language = resolve_language(cli.language, &config);

    // --data-dir: print path and exit
    if cli.data_dir {
        match SavedStats::path(language) {
            Some(p) => {
                if let Some(dir) = p.parent() {
                    println!("{}", dir.display());
                }
            }
            None => eprintln!("Could not determine data directory for this platform."),
        }
        return Ok(());
    }

    // --import: replay a keybr.com export into stats.json and exit
    if let Some(ref import_path) = cli.import {
        let target_wpm = cli.target_wpm.unwrap_or(config.target_wpm);
        let target_cpm = target_wpm as f64 * 5.0;
        import::run_import(import_path, target_cpm, cli.force, language)?;
        // Remembered only once the import succeeded, so the next plain
        // launch opens the language just imported.
        remember_language(cli.language, &mut config);
        return Ok(());
    }

    // --language is also remembered by a run that goes on to the TUI
    // (including --reset), never by --data-dir.
    remember_language(cli.language, &mut config);

    // --reset: delete stats file and continue fresh
    if cli.reset {
        match SavedStats::delete(language) {
            Ok(true) => println!("Stats reset. Starting fresh."),
            Ok(false) => println!("No stats file found. Starting fresh."),
            Err(e) => eprintln!("Warning: could not delete stats: {e}"),
        }
    }

    // Determine settings: CLI args override config file
    let target_wpm = cli.target_wpm.unwrap_or(config.target_wpm);

    let error_mode = if let Some(ref mode_str) = cli.error_mode {
        match mode_str.as_str() {
            "stop-on-error" | "stop" => ErrorMode::StopOnError,
            _ => ErrorMode::ForgiveMistakes,
        }
    } else {
        match config.error_mode {
            ErrorModeSerde::StopOnError => ErrorMode::StopOnError,
            ErrorModeSerde::ForgiveMistakes => ErrorMode::ForgiveMistakes,
        }
    };

    // Load saved stats (unless --reset was used)
    let saved_stats = if cli.reset {
        None
    } else {
        SavedStats::load(language)
    };

    // Bind the taria layer (agent accessibility) before entering the
    // alternate screen, so its report lands in the primary buffer. Unix
    // only: see `bind_taria_layer`.
    #[cfg(unix)]
    let mut layer = bind_taria_layer();

    let mut terminal = tui::init()?;

    let mut app = App::new_with_language(target_wpm, error_mode, saved_stats, language);
    app.fragment_length = config.fragment_length;
    app.natural_words = config.natural_words;
    app.daily_goal_minutes = config.daily_goal_minutes;
    app.alphabet_size = config.alphabet_size;
    app.scheduler.alphabet_size = config.alphabet_size;
    // Re-run the scheduler now that the configured alphabet_size is in
    // place: the constructor ran it with the default (0.0), so letters
    // force-unlocked by alphabet_size are not yet in `active_keys`. This
    // must happen BEFORE the pin normalization below, which drops any pin
    // not in `active_keys` — otherwise a valid pin on a forced letter
    // would be dropped and erased by the next config save. It also
    // refreshes the available drill list, which the pattern pin below is
    // validated against.
    app.run_scheduler_update();
    app.set_manual_focus_from_config(config.focus_pattern.as_deref(), config.focus_letter);
    app.generator.set_natural_words(app.natural_words);
    let rx = setup_event_channel();

    while app.running {
        // Agent inputs are drained around the blocking recv below; the
        // 50ms tick bounds the latency between an agent act and its
        // effect. `apply_agent_input` is as pure as `update` — it only
        // raises the same save flags, flushed at the bottom of the loop.
        #[cfg(unix)]
        drain_agent_input(&mut app, &layer);

        terminal.draw(|frame| view(&app, frame))?;

        // Publish the semantic tree for the exact state just drawn; the
        // layer dedups identical trees, so publishing every frame is free.
        #[cfg(unix)]
        layer.publish(tree::build_nodes(&app));

        match rx.recv() {
            Ok(event) => update(&mut app, event),
            Err(_) => break,
        }

        #[cfg(unix)]
        drain_agent_input(&mut app, &layer);

        // `update` never touches the disk — it only raises save flags.
        // Flushing here (including on the quit event, before the loop
        // exits) is the single place user files get written.
        app.flush_pending_saves();
    }

    tui::restore()?;

    // Only now the alternate screen is gone is printing safe again. These
    // four counters are agent traffic that went nowhere; the layer keeps
    // them precisely because it must not print them itself.
    #[cfg(unix)]
    report_taria_counters(&layer);

    Ok(())
}

/// Bind the taria layer (agent accessibility). Called before entering the
/// alternate screen, so whichever line it prints lands in the primary
/// buffer.
///
/// `bind_or_disabled` cannot fail: a layer that could not bind is inert and
/// answers every method, so the loop in `main` needs no branch on whether
/// taria came up, and the app can never refuse to start because of it. The
/// layer itself never prints — reporting is ours to place.
#[cfg(unix)]
fn bind_taria_layer() -> TariaLayer {
    let layer = TariaLayer::bind_or_disabled("keybr-tui");
    match layer.bind_error() {
        None => eprintln!(
            "keybr-tui: taria socket at {}",
            layer.socket_path().display()
        ),
        Some(err) => {
            eprintln!("keybr-tui: taria layer disabled ({err}); continuing without it")
        }
    }
    layer
}

/// Apply every queued agent input, refining the ack of each one the app
/// deliberately ignored.
///
/// The layer acks `Delivered` as it hands an input over, which says only
/// that this loop dequeued it. `Ignored` is the follow-up that tells an
/// agent waiting on an effect that none is coming — an act on the wrong
/// screen, an unknown node id, a key the grammar rejects, text sent while
/// no lesson is running. Last ack wins.
#[cfg(unix)]
fn drain_agent_input(app: &mut App, layer: &TariaLayer) {
    layer.drain_with_ids(|id, input| {
        if apply_agent_input(app, input) == Applied::Ignored {
            layer.ack(id, InputStatus::Ignored);
        }
    });
}

/// Report agent input that never reached the app (or whose answer never
/// reached the agent). Call after the terminal is restored.
#[cfg(unix)]
fn report_taria_counters(layer: &TariaLayer) {
    let dropped = layer.dropped_inputs();
    if dropped > 0 {
        eprintln!("keybr-tui: dropped {dropped} agent input(s): the app could not keep up");
    }
    let stale = layer.stale_inputs();
    if stale > 0 {
        eprintln!(
            "keybr-tui: discarded {stale} agent input(s): the bridge connection they arrived \
             on ended first"
        );
    }
    let unknown = layer.unknown_inputs();
    if unknown > 0 {
        eprintln!(
            "keybr-tui: could not read {unknown} agent input(s): the bridge speaks a newer \
             taria than this build; raise the taria dependency"
        );
    }
    let acks = layer.dropped_acks();
    if acks > 0 {
        eprintln!(
            "keybr-tui: lost the answer to {acks} agent input(s): the bridge read them slower \
             than the app answered, so those agent calls timed out instead"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(language: &str) -> Config {
        Config {
            language: language.to_string(),
            ..Config::default()
        }
    }

    #[test]
    fn language_flag_wins_over_config() {
        let tr = Language::Turkish;
        assert_eq!(resolve_language(Some(tr), &config_with("en")), tr);
        assert_eq!(resolve_language(None, &config_with("tr")), tr);
        // An unknown code in the file only warns and falls back.
        assert_eq!(
            resolve_language(None, &config_with("xx")),
            Language::English
        );
    }

    #[test]
    fn language_flag_lists_every_supported_code() {
        assert_eq!(parse_language("de"), Ok(Language::German));
        let err = parse_language("xx").unwrap_err();
        assert!(err.contains("en, tr, de"), "{err}");
        assert!(language_help().contains("en, tr, de"));
    }

    #[test]
    fn language_is_saved_only_when_the_flag_changes_it() {
        let (en, tr) = (Language::English, Language::Turkish);
        assert_eq!(language_to_save(Some(tr), &config_with("en")), Some("tr"));
        assert_eq!(language_to_save(Some(tr), &config_with("tr")), None);
        assert_eq!(language_to_save(Some(en), &config_with("en")), None);
        // The flag replaces a code this build does not know.
        assert_eq!(language_to_save(Some(en), &config_with("xx")), Some("en"));
        assert_eq!(language_to_save(None, &config_with("tr")), None);
    }
}
