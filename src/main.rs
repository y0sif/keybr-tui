mod app;
mod engine;
mod events;
mod metrics;
mod ui;
mod update;

use std::io::stdout;
use std::panic;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use events::setup_event_channel;
use ui::view;
use update::update;

fn main() -> anyhow::Result<()> {
    // Restore terminal if a panic occurs mid-session
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    setup_terminal()?;

    let result = run();

    restore_terminal()?;

    result
}

fn run() -> anyhow::Result<()> {
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let rx = setup_event_channel();

    while app.running {
        terminal.draw(|frame| view(&app, frame))?;

        match rx.recv() {
            Ok(event) => update(&mut app, event),
            Err(_) => break, // channel closed (threads died)
        }
    }

    Ok(())
}

fn setup_terminal() -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal() -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
