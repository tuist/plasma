use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::{io, time::Duration};

use crate::app::App;
use crate::input;
use crate::theme::{PROMPT_BORDER, PROMPT_PREFIX};

const SLASH_MENU_HEIGHT: u16 = 5;
const PROMPT_BORDER_HEIGHT: u16 = 3; // top border + content + bottom border
const FOOTER_HEIGHT: u16 = 2;
const FRAME_INTERVAL: Duration = Duration::from_millis(40);

pub fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal);
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    while !app.should_exit {
        let frame_start = std::time::Instant::now();

        // Pull any agent events the worker thread has produced since
        // the last frame so the document shows streamed output.
        app.poll_agent();

        terminal.draw(|frame| draw(frame, &app))?;

        // Drain any queued events without blocking so the loop keeps redrawing.
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if !input::handle_key(&mut app, key) {
                        app.should_exit = true;
                    }
                }
                _ => {}
            }
        }

        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_INTERVAL {
            std::thread::sleep(FRAME_INTERVAL - elapsed);
        }
    }
    Ok(())
}

/// One vertical slice of the TUI. The index into the layout's chunk
/// array is fixed by construction so the renderer never has to do
/// fragile arithmetic on the chunk count.
#[derive(Clone, Copy)]
enum Slot {
    Document,
    SlashMenu,
    ConnectMenu,
    Prompt,
    Footer,
}

fn build_layout(area: Rect, app: &App) -> (Rect, Vec<(Slot, Rect)>) {
    let in_menu = app.is_in_menu();
    let show_slash = app.should_show_commands() && !in_menu;
    let mut constraints: Vec<Constraint> = vec![Constraint::Min(0)];
    let mut slots: Vec<Slot> = vec![Slot::Document];
    if in_menu {
        constraints.push(Constraint::Length(app.menu_height()));
        slots.push(Slot::ConnectMenu);
    } else if show_slash {
        constraints.push(Constraint::Length(SLASH_MENU_HEIGHT));
        slots.push(Slot::SlashMenu);
    }
    // Guided menus (connect method, provider) suppress the prompt because
    // the menu itself is the input surface. The API-key mode keeps the
    // prompt so the user can type the key, and so does `Normal`.
    if !in_menu {
        constraints.push(Constraint::Length(PROMPT_BORDER_HEIGHT));
        slots.push(Slot::Prompt);
    }
    constraints.push(Constraint::Length(FOOTER_HEIGHT));
    slots.push(Slot::Footer);
    let chunks = Layout::vertical(constraints).split(area);
    let pairs = slots
        .into_iter()
        .zip(chunks.iter().copied())
        .collect();
    (area, pairs)
}

pub(crate) fn slots_for(app: &App, area: Rect) -> Vec<Slot> {
    let (_, pairs) = build_layout(area, app);
    pairs.into_iter().map(|(slot, _)| slot).collect()
}

fn draw(frame: &mut Frame, app: &App) {
    let (area, slots) = build_layout(frame.area(), app);
    let _ = area;

    // The document is always present.
    if let Some((_, area)) = slots.iter().find(|(slot, _)| matches!(slot, Slot::Document)) {
        frame.render_widget(Paragraph::new(app.document_lines()), *area);
    }

    // Connect-flow selection menu, if any.
    if let Some((_, area)) = slots
        .iter()
        .find(|(slot, _)| matches!(slot, Slot::ConnectMenu))
    {
        let menu_block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(PROMPT_BORDER);
        let inner = menu_block.inner(*area);
        frame.render_widget(menu_block, *area);
        frame.render_widget(Paragraph::new(app.menu_lines()), inner);
    }

    // Slash-command menu, if any.
    if let Some((_, area)) = slots
        .iter()
        .find(|(slot, _)| matches!(slot, Slot::SlashMenu))
    {
        frame.render_widget(Paragraph::new(app.slash_menu_lines()), *area);
    }

    // Prompt with top and bottom borders.
    if let Some((_, prompt_area)) = slots
        .iter()
        .find(|(slot, _)| matches!(slot, Slot::Prompt))
    {
        let prompt_block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(PROMPT_BORDER);
        let inner = prompt_block.inner(*prompt_area);
        let prompt_prefix = app.prompt_prefix();
        // While a request is in flight we render the "Thinking…"
        // message instead of the user's input — they already submitted
        // it, and showing the same text again would imply we hadn't
        // received it.
        let body: Line = if app.pending {
            Line::from(Span::styled(
                "Thinking\u{2026}",
                crate::theme::THINKING,
            ))
        } else {
            Line::from(vec![
                Span::styled(prompt_prefix.to_string(), PROMPT_PREFIX),
                Span::raw(app.input.clone()),
            ])
        };
        frame.render_widget(prompt_block, *prompt_area);
        frame.render_widget(Paragraph::new(body), inner);

        let cursor_x = if app.pending {
            // Hide the cursor while we wait for the worker.
            inner.x.saturating_sub(1)
        } else {
            (inner.x + prompt_prefix.len() as u16 + app.input.len() as u16)
                .min(inner.right().saturating_sub(1))
        };
        let cursor_y = inner.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // Footer.
    if let Some((_, footer_area)) = slots
        .iter()
        .find(|(slot, _)| matches!(slot, Slot::Footer))
    {
        let lines = app.footer.lines(footer_area.width);
        frame.render_widget(Paragraph::new(lines), *footer_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    fn has_slot(slots: &[Slot], target: Slot) -> bool {
        slots.iter().any(|slot| std::mem::discriminant(slot) == std::mem::discriminant(&target))
    }

    #[test]
    fn layout_in_normal_mode_has_document_prompt_and_footer() {
        let app = App::empty();
        let area = Rect::new(0, 0, 80, 24);
        let slots = slots_for(&app, area);
        assert!(has_slot(&slots, Slot::Document));
        assert!(has_slot(&slots, Slot::Prompt));
        assert!(has_slot(&slots, Slot::Footer));
        assert!(!has_slot(&slots, Slot::ConnectMenu));
    }

    #[test]
    fn layout_in_menu_mode_omits_the_prompt() {
        let mut app = App::empty();
        app.mode = crate::app::AppMode::AwaitingConnectMethod { selected: 0 };
        let area = Rect::new(0, 0, 80, 24);
        let slots = slots_for(&app, area);
        assert!(has_slot(&slots, Slot::Document));
        assert!(has_slot(&slots, Slot::ConnectMenu));
        assert!(has_slot(&slots, Slot::Footer));
        assert!(!has_slot(&slots, Slot::Prompt), "guided menu should hide the prompt");
    }

    #[test]
    fn layout_in_provider_menu_also_omits_the_prompt() {
        let mut app = App::empty();
        app.mode = crate::app::AppMode::AwaitingProvider { selected: 0 };
        let area = Rect::new(0, 0, 80, 24);
        let slots = slots_for(&app, area);
        assert!(!has_slot(&slots, Slot::Prompt), "provider menu should hide the prompt");
    }

    #[test]
    fn layout_in_api_key_mode_keeps_the_prompt() {
        let mut app = App::empty();
        app.mode = crate::app::AppMode::AwaitingApiKey {
            provider: "openrouter".into(),
            opened_browser: false,
        };
        let area = Rect::new(0, 0, 80, 24);
        let slots = slots_for(&app, area);
        assert!(has_slot(&slots, Slot::Prompt), "API key mode needs the prompt");
    }

    #[test]
    fn layout_with_slash_menu_has_a_bar_above_the_prompt() {
        let mut app = App::empty();
        app.input = "/".into();
        app.recompute_show_commands();
        let area = Rect::new(0, 0, 80, 24);
        let slots = slots_for(&app, area);
        let slash_index = slots
            .iter()
            .position(|slot| matches!(slot, Slot::SlashMenu))
            .expect("slash menu should be present");
        let prompt_index = slots
            .iter()
            .position(|slot| matches!(slot, Slot::Prompt))
            .expect("prompt should be present");
        assert!(slash_index < prompt_index, "slash menu must sit above the prompt");
    }
}
