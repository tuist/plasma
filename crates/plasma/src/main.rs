use std::{io, path::PathBuf, time::Duration};

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use plasma_openrouter::{OpenRouterInferenceProvider, save_key};
use plasma_session::Session;
use plasma_tools::workspace_files;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Parser)]
#[command(name = "plasma", about = "A Rust coding agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Connect {
        provider: String,
        #[arg(long)]
        api_key: String,
    },
}

struct App {
    session: Option<Session<OpenRouterInferenceProvider>>,
    input: String,
    document: Vec<Line<'static>>,
    show_commands: bool,
    should_exit: bool,
}

impl App {
    fn new() -> Self {
        let session = OpenRouterInferenceProvider::from_saved_key()
            .ok()
            .flatten()
            .map(Session::new);
        let mut document = vec![
            Line::from(Span::styled("Plasma", Style::default().fg(Color::Cyan))),
            Line::from("A Rust coding agent"),
            Line::from(""),
        ];
        if session.is_none() {
            document.push(Line::from("Use /connect to add an OpenRouter API key."));
        }
        Self {
            session,
            input: String::new(),
            document,
            show_commands: false,
            should_exit: false,
        }
    }

    fn submit(&mut self) {
        let input = std::mem::take(&mut self.input);
        self.show_commands = false;
        match input.trim() {
            "" => {}
            "/quit" => self.should_exit = true,
            "/help" => self.document.push(Line::from(
                "Commands: /connect, /files, /read <path>, /quit",
            )),
            "/connect" => self.document.push(Line::from(
                "Run `plasma connect openrouter --api-key <key>` to connect OpenRouter.",
            )),
            "/files" => match workspace_files(&PathBuf::from(".")) {
                Ok(files) => {
                    for file in files {
                        self.document.push(Line::from(file.display().to_string()));
                    }
                }
                Err(error) => self
                    .document
                    .push(Line::from(format!("Could not list files: {error}"))),
            },
            command if command.starts_with("/read ") => {
                match std::fs::read_to_string(command.trim_start_matches("/read ")) {
                    Ok(contents) => self.document.push(Line::from(contents)),
                    Err(error) => self
                        .document
                        .push(Line::from(format!("Could not read file: {error}"))),
                }
            }
            prompt => {
                self.document.push(Line::from(format!("> {prompt}")));
                match self.session.as_mut() {
                    Some(session) => match session.submit(prompt) {
                        Ok(message) => self.document.push(Line::from(message.content)),
                        Err(error) => self
                            .document
                            .push(Line::from(format!("Request failed: {error}"))),
                    },
                    None => self
                        .document
                        .push(Line::from("Connect OpenRouter first with /connect.")),
                }
            }
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.document.clone();
        if self.show_commands {
            lines.extend([
                Line::from(Span::styled(
                    "/connect   Connect OpenRouter",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from("/help      Show available commands"),
                Line::from("/files     List workspace files"),
                Line::from("/read      Read a workspace file"),
                Line::from("/quit      Exit Plasma"),
            ]);
        }
        lines.push(Line::from(format!("> {}", self.input)));
        lines
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Connect { provider, api_key }) = cli.command {
        anyhow::ensure!(
            provider.eq_ignore_ascii_case("openrouter"),
            "only OpenRouter is supported currently"
        );
        save_key(&api_key)?;
        println!("OpenRouter is connected.");
        return Ok(());
    }
    run_tui()
}

fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    while !app.should_exit {
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Paragraph::new(app.lines()), area);
            let cursor_x =
                (area.x + 2 + app.input.len() as u16).min(area.right().saturating_sub(1));
            let cursor_y =
                (area.y + app.lines().len() as u16 - 1).min(area.bottom().saturating_sub(1));
            frame.set_cursor_position((cursor_x, cursor_y));
        })?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('c')
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    app.should_exit = true
                }
                KeyCode::Char(character) => {
                    app.input.push(character);
                    app.show_commands = app.input.starts_with('/');
                }
                KeyCode::Backspace => {
                    app.input.pop();
                    app.show_commands = app.input.starts_with('/');
                }
                KeyCode::Enter => app.submit(),
                KeyCode::Esc => {
                    app.input.clear();
                    app.show_commands = false;
                }
                KeyCode::PageUp | KeyCode::PageDown => {}
                _ => {}
            }
        }
    }
    Ok(())
}
