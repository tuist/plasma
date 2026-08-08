use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
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

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let in_menu = app.is_in_menu();
    let show_slash = app.should_show_commands() && !in_menu;

    let mut constraints = vec![Constraint::Min(0)];
    if in_menu {
        constraints.push(Constraint::Length(app.menu_height()));
    } else {
        if show_slash {
            constraints.push(Constraint::Length(SLASH_MENU_HEIGHT));
        }
        constraints.push(Constraint::Length(PROMPT_BORDER_HEIGHT));
    }
    constraints.push(Constraint::Length(FOOTER_HEIGHT));
    let chunks = Layout::vertical(constraints).split(area);

    // Document history.
    frame.render_widget(Paragraph::new(app.document_lines()), chunks[0]);

    if in_menu {
        // The prompt is replaced by a navigable selection menu wrapped in
        // the same top/bottom border as the prompt so the layout still
        // reads as "input area".
        let menu_area = *chunks.last().expect("menu chunk is always present");
        let menu_block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(PROMPT_BORDER);
        let inner = menu_block.inner(menu_area);
        frame.render_widget(menu_block, menu_area);
        frame.render_widget(Paragraph::new(app.menu_lines()), inner);
        render_footer(frame, app, &chunks);
        return;
    }

    // Slash-command menu, if visible.
    if show_slash {
        let menu_index = chunks.len() - 2;
        frame.render_widget(Paragraph::new(app.slash_menu_lines()), chunks[menu_index]);
    }

    // Prompt with top and bottom borders so the input area is obvious.
    let prompt_area = chunks[chunks.len() - 2];
    let prompt_block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(PROMPT_BORDER);
    let inner = prompt_block.inner(prompt_area);
    let prompt_prefix = app.prompt_prefix();
    let styled_prompt = Line::from(vec![
        Span::styled(prompt_prefix.to_string(), PROMPT_PREFIX),
        Span::raw(app.input.clone()),
    ]);
    frame.render_widget(prompt_block, prompt_area);
    frame.render_widget(Paragraph::new(styled_prompt), inner);

    let cursor_x = (inner.x + prompt_prefix.len() as u16 + app.input.len() as u16)
        .min(inner.right().saturating_sub(1));
    let cursor_y = inner.y;
    frame.set_cursor_position((cursor_x, cursor_y));

    render_footer(frame, app, &chunks);
}

fn render_footer(frame: &mut Frame, app: &App, chunks: &[ratatui::layout::Rect]) {
    let footer_area = *chunks.last().expect("footer chunk is always present");
    let lines = app.footer.lines(footer_area.width);
    frame.render_widget(Paragraph::new(lines), footer_area);
}
