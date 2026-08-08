use ratatui::style::{Color, Modifier, Style};

pub const TITLE: Style = Style::new()
    .fg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
pub const SLASH_COMMAND: Style = Style::new().fg(Color::Cyan);
/// The currently highlighted entry in a menu. Bold cyan so the user can
/// spot it at a glance.
pub const SELECTED: Style = Style::new()
    .fg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
/// Navigation hints shown beneath menus (e.g. "↑↓ navigate ...").
pub const MENU_HINT: Style = Style::new().fg(Color::DarkGray);
pub const SUCCESS: Style = Style::new().fg(Color::Cyan);
pub const PROMPT_PREFIX: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const PROMPT_BORDER: Style = Style::new().fg(Color::DarkGray);
