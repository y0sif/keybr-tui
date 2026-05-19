mod app;
mod components;
mod config;
mod engine;
mod events;
mod metrics;
mod persistence;
mod tui;
mod ui;
mod update;

use clap::Parser;

use app::{App, ErrorMode};
use config::{Config, ErrorModeSerde};
use events::setup_event_channel;
use persistence::SavedStats;
use ui::view;
use update::update;

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

    let mut terminal = tui::init()?;

    let mut app = App::new_with_state(target_wpm, error_mode, saved_stats);
    app.fragment_length = config.fragment_length;
    app.natural_words = config.natural_words;
    app.generator.set_natural_words(app.natural_words);
    let rx = setup_event_channel();

    while app.running {
        terminal.draw(|frame| view(&app, frame))?;

        match rx.recv() {
            Ok(event) => update(&mut app, event),
            Err(_) => break,
        }
    }

    tui::restore()?;

    Ok(())
}
