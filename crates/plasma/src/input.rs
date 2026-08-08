use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode};

/// Returns true if the app should keep running, false if the user asked to quit.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return handle_ctrl_c(app);
    }
    if key.code == KeyCode::Esc {
        return handle_escape(app);
    }
    match key.code {
        KeyCode::Char(character) => {
            app.input.push(character);
            app.show_commands = app.is_normal() && app.input.starts_with('/');
        }
        KeyCode::Backspace => {
            app.input.pop();
            app.show_commands = app.is_normal() && app.input.starts_with('/');
        }
        KeyCode::Enter => app.submit(),
        KeyCode::PageUp | KeyCode::PageDown => {}
        _ => {}
    }
    true
}

/// Esc clears the input if there is something to clear, cancels an ongoing
/// flow (e.g. the API key prompt), or does nothing otherwise.
fn handle_escape(app: &mut App) -> bool {
    if !app.input.is_empty() {
        app.input.clear();
        app.show_commands = false;
        return true;
    }
    if !app.is_normal() {
        app.mode = AppMode::Normal;
        app.push_info("Connection cancelled.");
        return true;
    }
    true
}

/// Ctrl+C behaves like Esc when there is something to clear or an ongoing flow,
/// and only exits when there is nothing pending.
fn handle_ctrl_c(app: &mut App) -> bool {
    if !app.input.is_empty() {
        app.input.clear();
        app.show_commands = false;
        return true;
    }
    if !app.is_normal() {
        app.mode = AppMode::Normal;
        app.push_info("Connection cancelled.");
        return true;
    }
    false
}
