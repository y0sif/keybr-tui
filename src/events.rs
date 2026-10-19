use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::crossterm::terminal;

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    /// The terminal was resized; carries its new height in rows, as
    /// ratatui will draw it.
    Resize(u16),
}

pub fn setup_event_channel() -> Receiver<AppEvent> {
    let (tx, rx) = mpsc::channel::<AppEvent>();

    // Tick thread: fires every 50ms to drive WPM updates
    let tx_tick = tx.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(50));
        if tx_tick.send(AppEvent::Tick).is_err() {
            break;
        }
    });

    // Input thread: blocks on crossterm events and forwards them
    thread::spawn(move || loop {
        match event::read() {
            Ok(Event::Key(key))
                // Only process key press events, not release/repeat on some backends
                if key.kind == KeyEventKind::Press && tx.send(AppEvent::Key(key)).is_err() => {
                    break;
                }
            // The height comes from `terminal::size()`, the call ratatui
            // sizes each frame with, so the Progress scroll limits always
            // match the frame that is drawn; the event's rows are only the
            // fallback.
            Ok(Event::Resize(_, rows))
                if tx
                    .send(AppEvent::Resize(
                        terminal::size().map_or(rows, |(_, height)| height),
                    ))
                    .is_err() => {
                    break;
                }
            _ => {}
        }
    });

    rx
}
