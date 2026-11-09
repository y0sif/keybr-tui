use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppScreen};
use crate::events::AppEvent;

// The agent-input half of this module is unix-only: taria's transport is a
// unix domain socket, so `taria-ratatui` is a `cfg(unix)` dependency and
// `crate::tree` does not exist on Windows. Keyboard handling below is
// untouched and identical on every platform.
#[cfg(unix)]
mod agent;
mod menu;
mod progress;
mod settings;
#[cfg(test)]
mod test_support;
mod typing;

#[cfg(unix)]
pub use agent::{apply_agent_input, Applied};
use menu::handle_menu_key;
use progress::handle_progress_key;
use settings::handle_settings_key;
use typing::handle_typing_key;

/// Request a stats save. `update` never touches the disk itself — main's
/// event loop queues the actual write after the event is processed.
/// This keeps the MVU update layer pure and, crucially, stops unit tests
/// that drive `update` from clobbering the user's real stats file.
fn request_stats_save(app: &mut App) {
    app.pending_stats_save = true;
}

/// Request a config save (see `request_stats_save` for why it's deferred).
fn request_config_save(app: &mut App) {
    app.pending_config_save = true;
}

pub fn update(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Key(key) => handle_key(app, key),
        AppEvent::Tick => {}
        AppEvent::Resize(height) => app.terminal_height = height,
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    use KeyCode::*;

    // Global: Ctrl+C always quits
    if key.code == Char('c') && key.modifiers == KeyModifiers::CONTROL {
        request_stats_save(app);
        app.running = false;
        return;
    }

    match app.screen {
        AppScreen::Menu => handle_menu_key(app, key),
        AppScreen::Typing => handle_typing_key(app, key),
        AppScreen::Progress => handle_progress_key(app, key),
        AppScreen::Settings => handle_settings_key(app, key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_records_the_terminal_height() {
        let mut app = App::new();
        update(&mut app, AppEvent::Resize(42));
        assert_eq!(app.terminal_height, 42);
    }
}
