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
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    // --data-dir: print path and exit
    if cli.data_dir {
        match SavedStats::path() {
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
        let config = Config::load();
        let target_wpm = cli.target_wpm.unwrap_or(config.target_wpm);
        let target_cpm = target_wpm as f64 * 5.0;
        return import::run_import(import_path, target_cpm, cli.force);
    }

    // --reset: delete stats file and continue fresh
    if cli.reset {
        match SavedStats::delete() {
            Ok(true) => println!("Stats reset. Starting fresh."),
            Ok(false) => println!("No stats file found. Starting fresh."),
            Err(e) => eprintln!("Warning: could not delete stats: {e}"),
        }
    }

    // Load config from disk
    let config = Config::load();

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
    let saved_stats = if cli.reset { None } else { SavedStats::load() };

    // Bind the taria layer (agent accessibility) before entering the
    // alternate screen, so its report lands in the primary buffer. Unix
    // only: see `bind_taria_layer`.
    #[cfg(unix)]
    let mut layer = bind_taria_layer();

    let mut terminal = tui::init()?;

    let mut app = App::new_with_state(target_wpm, error_mode, saved_stats);
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
    // would be dropped and erased by the next config save.
    app.scheduler.update(&app.per_key_stats, app.target_cpm);
    app.set_manual_focus_from_config(config.focus_letter);
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
