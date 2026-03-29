mod app;
mod engine;
mod events;
mod metrics;
mod tui;
mod ui;
mod update;

use clap::Parser;

use app::{App, ErrorMode};
use events::setup_event_channel;
use ui::view;
use update::update;

/// A terminal typing trainer inspired by keybr.com with adaptive learning.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Target typing speed in words per minute.
    #[arg(long, default_value_t = 30)]
    target_wpm: u32,

    /// Error handling mode: "move-on" or "stop-on-error".
    #[arg(long, default_value = "move-on")]
    error_mode: String,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    let error_mode = match cli.error_mode.as_str() {
        "stop-on-error" | "stop" => ErrorMode::StopOnError,
        _ => ErrorMode::ForgiveMistakes,
    };

    let mut terminal = tui::init()?;

    let mut app = App::new_with_opts(cli.target_wpm, error_mode);
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
