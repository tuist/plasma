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
/// Block around a tool call or tool result in the document, modelled on
/// gPi's plugin-formatted output.
pub const TOOL_BLOCK_BG: Style = Style::new().bg(Color::Rgb(20, 40, 30));
/// Tool name printed at the top of a tool block (e.g. `read` / `bash`).
pub const TOOL_NAME: Style = Style::new()
    .fg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
/// Body text inside a tool block. Italic so it reads like gPi's model
/// commentary; for tool output the host also dims it slightly.
pub const TOOL_BODY: Style = Style::new()
    .fg(Color::Gray)
    .add_modifier(Modifier::ITALIC);
/// Body text for the model's free-form reasoning (the italic lines
/// between tool calls in gPi).
pub const AGENT_TEXT: Style = Style::new()
    .fg(Color::Gray)
    .add_modifier(Modifier::ITALIC);
/// Tool result body — slightly dimmer than agent text so the result
/// reads as data the model consumed rather than a new thought.
pub const TOOL_RESULT: Style = Style::new().fg(Color::DarkGray);
/// Error text inside a tool result block.
pub const TOOL_ERROR: Style = Style::new().fg(Color::Red);
