use std::path::PathBuf;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use plasma_openrouter::{OpenRouterInferenceProvider, save_key};
use plasma_session::Session;
use plasma_tools::workspace_files;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::animation::PlasmaField;
use crate::theme::{SLASH_COMMAND, SUCCESS, TITLE};

#[derive(Clone, Copy)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "connect", description: "Connect OpenRouter" },
    SlashCommand { name: "files", description: "List workspace files" },
    SlashCommand { name: "help", description: "Show available commands" },
    SlashCommand { name: "quit", description: "Exit Plasma" },
    SlashCommand { name: "read", description: "Read a workspace file" },
];

#[derive(Clone)]
pub enum AppMode {
    Normal,
    AwaitingApiKey { provider: String },
}

pub struct App {
    pub session: Option<Session<OpenRouterInferenceProvider>>,
    pub input: String,
    pub document: Vec<Line<'static>>,
    pub mode: AppMode,
    pub show_commands: bool,
    pub should_exit: bool,
    pub plasma: PlasmaField,
}

impl App {
    pub fn new() -> Self {
        let session = OpenRouterInferenceProvider::from_saved_key()
            .ok()
            .flatten()
            .map(Session::new);
        let mut document = vec![
            Line::from(Span::styled("Plasma", TITLE)),
            Line::from("A Rust coding agent"),
            Line::from(""),
        ];
        if session.is_none() {
            document.push(Line::from(
                "Type a prompt to start. You'll be asked for an OpenRouter API key on first use.",
            ));
        }
        Self {
            session,
            input: String::new(),
            document,
            mode: AppMode::Normal,
            show_commands: false,
            should_exit: false,
            plasma: PlasmaField::new(),
        }
    }

    pub fn prompt_prefix(&self) -> &'static str {
        match self.mode {
            AppMode::Normal => "> ",
            AppMode::AwaitingApiKey { .. } => "OpenRouter API key: ",
        }
    }

    pub fn is_normal(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    pub fn document_lines(&self) -> Vec<Line<'static>> {
        self.document.clone()
    }

    pub fn slash_menu_lines(&self) -> Vec<Line<'static>> {
        let query = self.input.trim_start_matches('/');
        let commands = filter_slash_commands(query);
        commands
            .into_iter()
            .map(|command| {
                Line::from(format!(
                    "/{:<10} {}",
                    command.name, command.description
                ))
                .style(Style::default())
                .patch_style(SLASH_COMMAND)
            })
            .collect()
    }

    pub fn should_show_commands(&self) -> bool {
        self.show_commands && self.is_normal()
    }

    pub fn push_prompt_echo(&mut self, prompt: &str) {
        self.document.push(Line::from(format!("> {prompt}")));
    }

    pub fn push_info(&mut self, message: impl Into<String>) {
        self.document.push(Line::from(message.into()));
    }

    pub fn submit(&mut self) {
        let input = std::mem::take(&mut self.input);
        self.show_commands = false;
        match &self.mode {
            AppMode::AwaitingApiKey { provider } => {
                let provider = provider.clone();
                let key = input.trim();
                if key.is_empty() {
                    self.document
                        .push(Line::from("API key cannot be empty. Press Esc to cancel."));
                    return;
                }
                self.connect(&provider, key);
            }
            AppMode::Normal => match input.trim() {
                "" => {}
                "/quit" => self.should_exit = true,
                "/help" => self.document.push(Line::from(
                    "Commands: /connect, /files, /read <path>, /quit",
                )),
                "/connect" => {
                    if self.session.is_some() {
                        self.document
                            .push(Line::from("OpenRouter is already connected."));
                    } else {
                        self.begin_connect();
                    }
                }
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
                    if self.session.is_none() {
                        self.begin_connect();
                        return;
                    }
                    self.push_prompt_echo(prompt);
                    self.send_prompt(prompt);
                }
            },
        }
    }

    fn send_prompt(&mut self, prompt: &str) {
        match self.session.as_mut() {
            Some(session) => match session.submit(prompt) {
                Ok(message) => self.document.push(Line::from(message.content)),
                Err(error) => self
                    .document
                    .push(Line::from(format!("Request failed: {error}"))),
            },
            None => unreachable!("send_prompt is only called when a session exists"),
        }
    }

    fn begin_connect(&mut self) {
        self.document.push(Line::from(
            "OpenRouter is not connected yet. Paste your API key below (Esc to cancel).",
        ));
        self.mode = AppMode::AwaitingApiKey {
            provider: "openrouter".to_string(),
        };
    }

    fn connect(&mut self, provider: &str, key: &str) {
        if !provider.eq_ignore_ascii_case("openrouter") {
            self.document
                .push(Line::from(format!("Unknown provider: {provider}")));
            self.mode = AppMode::Normal;
            return;
        }
        if let Err(error) = save_key(key) {
            self.document
                .push(Line::from(format!("Failed to save key: {error}")));
            self.mode = AppMode::Normal;
            return;
        }
        match OpenRouterInferenceProvider::from_saved_key() {
            Ok(Some(provider)) => {
                self.session = Some(Session::new(provider));
                self.mode = AppMode::Normal;
                self.document
                    .push(Line::from(Span::styled("OpenRouter connected.", SUCCESS)));
            }
            _ => {
                self.document
                    .push(Line::from("Saved the key, but could not load the provider."));
                self.mode = AppMode::Normal;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn filter_slash_commands(query: &str) -> Vec<SlashCommand> {
    if query.is_empty() {
        return SLASH_COMMANDS.to_vec();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, SlashCommand)> = SLASH_COMMANDS
        .iter()
        .copied()
        .filter_map(|command| {
            let candidate = format!("/{} {}", command.name, command.description);
            matcher
                .fuzzy_match(&candidate, query)
                .map(|score| (score, command))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored.into_iter().map(|(_, command)| command).collect()
}
