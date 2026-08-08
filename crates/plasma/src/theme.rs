use ratatui::style::{Color, Modifier, Style};

pub const TITLE: Style = Style::new().fg(Color::Cyan);
pub const SLASH_COMMAND: Style = Style::new().fg(Color::Cyan);
pub const SUCCESS: Style = Style::new().fg(Color::Cyan);
pub const PROMPT_PREFIX: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const PROMPT_BORDER: Style = Style::new().fg(Color::DarkGray);
