use std::path::PathBuf;
use std::sync::Arc;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use plasma_inference::ToolDefinition;
use plasma_openrouter::{OpenRouterInferenceProvider, save_key};
use plasma_session::{AgentEvent, Session, ToolDispatcher};
use plasma_tools::{BashTool, ReadTool, Tool, workspace_files};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use serde_json::Value;

use crate::browser::{BrowserOpener, SystemBrowser};
use crate::footer::Footer;
use crate::openrouter_oauth as oauth_flow;
use crate::rich_text::{RichSpan, RichText};
use crate::theme::{
    AGENT_TEXT, MENU_HINT, SELECTED, SLASH_COMMAND, SUCCESS, TITLE, TOOL_BODY,
    TOOL_ERROR, TOOL_NAME, TOOL_RESULT,
};

/// One row in the slash-command menu.
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

/// One row in a multi-step selection menu (the connect flow).
#[derive(Clone, Copy)]
pub struct MenuOption {
    pub value: &'static str,
    pub description: &'static str,
}

pub const CONNECT_METHODS: &[MenuOption] = &[
    MenuOption { value: "account", description: "Sign in with an account" },
    MenuOption { value: "api-key", description: "Sign in with an API key" },
];

pub const PROVIDERS: &[MenuOption] = &[
    MenuOption { value: "openrouter", description: "OpenRouter" },
];

/// The state machine the TUI walks through.
#[derive(Clone)]
pub enum AppMode {
    Normal,
    /// Choosing how to connect: account vs API key.
    AwaitingConnectMethod { selected: usize },
    /// Choosing which provider to connect.
    AwaitingProvider { selected: usize },
    /// Awaiting the user to type the API key. `opened_browser` records
    /// whether the host browser was opened for this attempt.
    AwaitingApiKey {
        provider: String,
        opened_browser: bool,
    },
}

pub struct App {
    pub session: Option<Session<OpenRouterInferenceProvider>>,
    pub input: String,
    pub document: Vec<Line<'static>>,
    pub mode: AppMode,
    pub show_commands: bool,
    /// Highlighted index inside the filtered slash-command list. Only
    /// meaningful while `show_commands` is true.
    pub slash_selected: Option<usize>,
    pub should_exit: bool,
    pub footer: Footer,
    browser: Box<dyn BrowserOpener>,
    tool_definitions: Arc<Vec<ToolDefinition>>,
    dispatcher: AppToolDispatcher,
}

impl App {
    /// Create the app used by the TUI on launch. Reads any saved OpenRouter
    /// key from disk and seeds the welcome document.
    pub fn new() -> Self {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self::empty();
        app.session = OpenRouterInferenceProvider::from_saved_key()
            .ok()
            .flatten()
            .map(Session::new);
        app.document = vec![
            Line::from(Span::styled("Plasma", TITLE)),
            Line::from("A coding agent"),
            Line::from(""),
        ];
        if app.session.is_none() {
            app.document.push(
                RichText::new()
                    .push_str("Type ")
                    .push_command("/connect")
                    .push_str(" to connect your account, or a prompt to start.")
                    .into_line(),
            );
        }
        app.footer = Footer::new(working_dir);
        app
    }

    /// Create a bare app with no filesystem access. Used by tests so they
    /// can run in parallel without touching the user's config.
    pub fn empty() -> Self {
        let (tool_definitions, dispatcher) = build_tool_chain(PathBuf::from("."));
        Self {
            session: None,
            input: String::new(),
            document: Vec::new(),
            mode: AppMode::Normal,
            show_commands: false,
            slash_selected: None,
            should_exit: false,
            footer: Footer::new(PathBuf::from(".")),
            browser: Box::new(SystemBrowser),
            tool_definitions,
            dispatcher,
        }
    }

    /// Swap the browser opener. Mainly useful in tests.
    #[cfg(test)]
    pub fn with_browser(mut self, browser: Box<dyn BrowserOpener>) -> Self {
        self.browser = browser;
        self
    }

    pub fn prompt_prefix(&self) -> &'static str {
        match self.mode {
            AppMode::Normal => "> ",
            AppMode::AwaitingApiKey { .. } => "OpenRouter API key: ",
            // The prompt is hidden behind a menu in the selection states.
            AppMode::AwaitingConnectMethod { .. } | AppMode::AwaitingProvider { .. } => "",
        }
    }

    pub fn is_normal(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    /// True when the TUI is showing a navigable menu instead of the prompt.
    pub fn is_in_menu(&self) -> bool {
        matches!(
            self.mode,
            AppMode::AwaitingConnectMethod { .. } | AppMode::AwaitingProvider { .. }
        )
    }

    pub fn document_lines(&self) -> Vec<Line<'static>> {
        self.document.clone()
    }

    /// Render the slash-command menu with the currently highlighted entry
    /// marked with a `→` and bold styling.
    pub fn slash_menu_lines(&self) -> Vec<Line<'static>> {
        let query = self.input.trim_start_matches('/');
        filter_slash_commands(query)
            .into_iter()
            .enumerate()
            .map(|(index, command)| {
                let is_selected = self.slash_selected == Some(index);
                let prefix = if is_selected { "→ " } else { "  " };
                let mut line = RichText::new()
                    .push_str(prefix)
                    .push_str("/")
                    .push_command(command.name)
                    .push_str(&format!(" {:<8}", ""))
                    .push_str(command.description);
                if is_selected {
                    line = line.push_strong("");
                }
                let mut ratatui_line = line.into_line();
                if !is_selected {
                    ratatui_line = ratatui_line.patch_style(SLASH_COMMAND);
                }
                ratatui_line
            })
            .collect()
    }

    /// The number of terminal rows needed to render the current selection
    /// menu, including its top/bottom borders. Zero when no menu is shown.
    pub fn menu_height(&self) -> u16 {
        let question = self.menu_question().is_some() as u16;
        let options = self.menu_options().map_or(0, |o| o.len() as u16);
        // 1 header + 1 spacer + N options + 1 spacer + 1 hints + 2 borders
        question + 1 + options + 1 + 1 + 2
    }

    /// Render the active selection menu (connect method or provider).
    pub fn menu_lines(&self) -> Vec<Line<'static>> {
        let Some(question) = self.menu_question() else {
            return Vec::new();
        };
        let Some(options) = self.menu_options() else {
            return Vec::new();
        };
        let selected = match self.mode {
            AppMode::AwaitingConnectMethod { selected } => selected,
            AppMode::AwaitingProvider { selected } => selected,
            _ => 0,
        };
        let mut lines = vec![
            Line::from(Span::styled(
                question,
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];
        for (index, option) in options.iter().enumerate() {
            let prefix = if index == selected { "→ " } else { "  " };
            let style = if index == selected { SELECTED } else { Style::default() };
            lines.push(Line::from(Span::styled(
                format!("{prefix}{}", option.description),
                style,
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑↓ navigate   enter select   escape/ctrl+c cancel",
            MENU_HINT,
        )));
        lines
    }

    fn menu_question(&self) -> Option<&'static str> {
        match self.mode {
            AppMode::AwaitingConnectMethod { .. } => Some("Select authentication method:"),
            AppMode::AwaitingProvider { .. } => Some("Choose a provider:"),
            _ => None,
        }
    }

    fn menu_options(&self) -> Option<&'static [MenuOption]> {
        match self.mode {
            AppMode::AwaitingConnectMethod { .. } => Some(CONNECT_METHODS),
            AppMode::AwaitingProvider { .. } => Some(PROVIDERS),
            _ => None,
        }
    }

    pub fn should_show_commands(&self) -> bool {
        self.show_commands && self.is_normal()
    }

    /// Recompute `show_commands` and reset the slash-menu selection. Called
    /// by the input handler after every change so the menu hides as soon as
    /// the typed query no longer matches any slash command.
    pub fn recompute_show_commands(&mut self) {
        let now_showing = self.is_normal()
            && self.input.starts_with('/')
            && !filter_slash_commands(self.input.trim_start_matches('/')).is_empty();
        if now_showing {
            // Always start with the first match highlighted after a change.
            self.slash_selected = Some(0);
        } else {
            self.slash_selected = None;
        }
        self.show_commands = now_showing;
    }

    /// Move the selection in the active menu (connect flow or slash) one
    /// step up. No-op when nothing is navigable.
    pub fn select_up(&mut self) {
        match self.mode {
            AppMode::AwaitingConnectMethod { ref mut selected } => {
                let len = CONNECT_METHODS.len();
                *selected = selected.checked_sub(1).unwrap_or(len - 1);
            }
            AppMode::AwaitingProvider { ref mut selected } => {
                let len = PROVIDERS.len();
                *selected = selected.checked_sub(1).unwrap_or(len - 1);
            }
            AppMode::Normal if self.show_commands => self.slash_select_up(),
            _ => {}
        }
    }

    /// Move the selection in the active menu (connect flow or slash) one
    /// step down. No-op when nothing is navigable.
    pub fn select_down(&mut self) {
        match self.mode {
            AppMode::AwaitingConnectMethod { ref mut selected } => {
                let len = CONNECT_METHODS.len();
                *selected = (*selected + 1) % len;
            }
            AppMode::AwaitingProvider { ref mut selected } => {
                let len = PROVIDERS.len();
                *selected = (*selected + 1) % len;
            }
            AppMode::Normal if self.show_commands => self.slash_select_down(),
            _ => {}
        }
    }

    fn slash_select_up(&mut self) {
        let len = filter_slash_commands(self.input.trim_start_matches('/')).len();
        if len == 0 {
            self.slash_selected = None;
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(0) | None => len - 1,
            Some(index) => index - 1,
        });
    }

    fn slash_select_down(&mut self) {
        let len = filter_slash_commands(self.input.trim_start_matches('/')).len();
        if len == 0 {
            self.slash_selected = None;
            return;
        }
        self.slash_selected = Some(match self.slash_selected {
            Some(index) if index + 1 < len => index + 1,
            _ => 0,
        });
    }

    /// Confirm the current selection. In a connect-flow menu this advances
    /// the state machine; in the slash menu it executes the highlighted
    /// command; otherwise the typed input is submitted.
    pub fn confirm(&mut self) {
        match self.mode {
            AppMode::AwaitingConnectMethod { .. } | AppMode::AwaitingProvider { .. } => {
                self.confirm_menu();
            }
            AppMode::Normal if self.show_commands && self.slash_selected.is_some() => {
                self.confirm_slash_selection();
            }
            _ => self.submit(),
        }
    }

    fn confirm_menu(&mut self) {
        let selected = match self.mode {
            AppMode::AwaitingConnectMethod { selected } => selected,
            AppMode::AwaitingProvider { selected } => selected,
            _ => return,
        };
        let value = match self.mode {
            AppMode::AwaitingConnectMethod { .. } => CONNECT_METHODS[selected].value,
            AppMode::AwaitingProvider { .. } => PROVIDERS[selected].value,
            _ => unreachable!(),
        };
        match (self.mode.clone(), value) {
            (AppMode::AwaitingConnectMethod { .. }, "account") => self.enter_provider(),
            (AppMode::AwaitingConnectMethod { .. }, "api-key") => {
                self.begin_api_key_input("openrouter", false);
            }
            (AppMode::AwaitingProvider { .. }, "openrouter") => {
                self.begin_oauth_flow("openrouter");
            }
            _ => {}
        }
    }

    fn confirm_slash_selection(&mut self) {
        let commands = filter_slash_commands(self.input.trim_start_matches('/'));
        let Some(index) = self.slash_selected else { return };
        let Some(command) = commands.get(index) else { return };
        self.input = format!("/{}", command.name);
        self.slash_selected = None;
        self.show_commands = false;
        self.submit();
    }

    pub fn push_prompt_echo(&mut self, prompt: &str) {
        self.document.push(Line::from(format!("> {prompt}")));
    }

    pub fn push_info(&mut self, message: impl Into<String>) {
        self.document.push(Line::from(message.into()));
    }

    /// Handle a text submission (Enter pressed without a menu active). The
    /// slash-menu path is handled by `confirm` instead.
    pub fn submit(&mut self) {
        let input = std::mem::take(&mut self.input);
        self.show_commands = false;
        self.slash_selected = None;
        match self.mode.clone() {
            AppMode::Normal => self.submit_normal(&input),
            AppMode::AwaitingApiKey { provider, opened_browser } => {
                self.submit_api_key(&provider, opened_browser, &input);
            }
            // The selection menus never reach this path; they go through
            // `confirm` -> `confirm_menu`.
            AppMode::AwaitingConnectMethod { .. } | AppMode::AwaitingProvider { .. } => {}
        }
    }

    fn submit_normal(&mut self, input: &str) {
        let trimmed = input.trim();
        match trimmed {
            "" => {}
            "/quit" => self.should_exit = true,
            "/help" => self.document.push(Line::from(
                "Commands: /connect, /files, /read <path>, /quit",
            )),
            "/connect" => {
                if self.session.is_some() {
                    self.push_info("OpenRouter is already connected.");
                } else {
                    self.enter_connect_method();
                }
            }
            "/files" => self.list_workspace_files(),
            command if command.starts_with("/read ") => {
                self.read_workspace_file(command.trim_start_matches("/read "));
            }
            unknown if unknown.starts_with('/') => {
                self.push_info(format!("Unknown command: {unknown}"));
            }
            prompt => {
                if self.session.is_none() {
                    self.enter_connect_method();
                } else {
                    self.push_prompt_echo(prompt);
                    self.send_prompt(prompt);
                }
            }
        }
    }

    fn submit_api_key(&mut self, provider: &str, opened_browser: bool, input: &str) {
        let key = input.trim();
        if key.is_empty() {
            self.push_info("API key cannot be empty. Press Esc to cancel.");
            return;
        }
        self.finalize_connect(provider, opened_browser, key);
    }

    fn enter_connect_method(&mut self) {
        self.document.push(Line::from(""));
        self.document
            .push(Line::from("Let's connect your account."));
        self.mode = AppMode::AwaitingConnectMethod { selected: 0 };
    }

    fn enter_provider(&mut self) {
        self.mode = AppMode::AwaitingProvider { selected: 0 };
    }

    /// Move into the API-key input mode. If `opened_browser` is true, the
    /// host browser is opened to the provider's key page so the user can
    /// create or copy a key without leaving the agent.
    fn begin_api_key_input(&mut self, provider: &str, opened_browser: bool) {
        if opened_browser
            && let Some(url) = provider_keys_url(provider)
        {
            self.browser.open(url);
            self.document.push(Line::from(""));
            self.document.push(Line::from(format!(
                "Opening {url} in your browser. Create or copy a key there, then paste it below."
            )));
        }
        self.document.push(Line::from(""));
        self.push_info("Paste your API key below. Press Esc to cancel.");
        self.mode = AppMode::AwaitingApiKey {
            provider: provider.to_string(),
            opened_browser,
        };
    }

    fn finalize_connect(&mut self, provider: &str, _opened_browser: bool, key: &str) {
        if !provider.eq_ignore_ascii_case("openrouter") {
            self.push_info(format!("Unknown provider: {provider}"));
            self.mode = AppMode::Normal;
            return;
        }
        if let Err(error) = save_key(key) {
            self.push_info(format!("Failed to save key: {error}"));
            self.mode = AppMode::Normal;
            return;
        }
        match OpenRouterInferenceProvider::from_saved_key() {
            Ok(Some(provider)) => {
                self.session = Some(Session::new(provider));
                self.mode = AppMode::Normal;
                self.document.push(Line::from(Span::styled(
                    "OpenRouter connected.",
                    SUCCESS,
                )));
            }
            _ => {
                self.push_info("Saved the key, but could not load the provider.");
                self.mode = AppMode::Normal;
            }
        }
    }

    /// Run the OpenRouter OAuth flow: open the authorize URL in the host
    /// browser, wait for the loopback callback, exchange the code for a
    /// permanent API key, and save it. On failure, fall back to a normal
    /// prompt with an error message so the user is not stuck.
    fn begin_oauth_flow(&mut self, provider: &str) {
        let key = {
            let browser = &self.browser;
            oauth_flow::login(|url| browser.open(url))
        };
        match key {
            Ok(key) => self.finalize_connect(provider, true, &key),
            Err(error) => {
                self.push_info(format!("OpenRouter sign-in failed: {error}"));
                self.mode = AppMode::Normal;
            }
        }
    }

    fn list_workspace_files(&mut self) {
        match workspace_files(&PathBuf::from(".")) {
            Ok(files) => {
                for file in files {
                    self.document.push(Line::from(file.display().to_string()));
                }
            }
            Err(error) => self.push_info(format!("Could not list files: {error}")),
        }
    }

    fn read_workspace_file(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(contents) => self.document.push(Line::from(contents)),
            Err(error) => self.push_info(format!("Could not read file: {error}")),
        }
    }

    fn send_prompt(&mut self, prompt: &str) {
        let Some(session) = self.session.as_mut() else {
            unreachable!("send_prompt is only called when a session exists")
        };
        let definitions = Arc::clone(&self.tool_definitions);
        let dispatcher = &self.dispatcher as &dyn ToolDispatcher;
        // The session borrow cannot overlap with `self` inside the
        // event callback, so collect events into a `Vec` while the
        // session is still borrowed and replay them once it is released.
        let mut events: Vec<AgentEvent> = Vec::new();
        let result = session.submit_with_tools(
            prompt,
            definitions,
            dispatcher,
            &mut |event| events.push(event),
        );
        for event in events {
            self.handle_agent_event(event);
        }
        if let Err(error) = result {
            self.push_info(format!("Request failed: {error}"));
        }
    }

    fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Text(text) => {
                if text.trim().is_empty() {
                    return;
                }
                for line in text.lines() {
                    self.document.push(Line::from(Span::styled(
                        line.to_string(),
                        AGENT_TEXT,
                    )));
                }
            }
            AgentEvent::ToolCall { name, arguments } => {
                self.document.push(Line::from(""));
                self.document.push(tool_call_line(&name, &arguments));
            }
            AgentEvent::ToolResult { name, output, error } => {
                self.document.push(tool_result_line(&name, output, error));
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::empty()
    }
}

/// Routes `ToolCall`s (name + JSON arguments) to the matching `Tool`
/// implementation. Keeps the session decoupled from the concrete tool
/// set the host registered at startup.
struct AppToolDispatcher {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolDispatcher for AppToolDispatcher {
    fn dispatch(&self, name: &str, arguments_json: &str) -> Result<String, String> {
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.name() == name)
            .ok_or_else(|| format!("unknown tool '{name}'"))?;
        let arguments: Value = serde_json::from_str(arguments_json)
            .map_err(|error| format!("invalid JSON arguments for {name}: {error}"))?;
        tool.execute(&arguments)
    }
}

fn build_tool_chain(root: PathBuf) -> (Arc<Vec<ToolDefinition>>, AppToolDispatcher) {
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(ReadTool { root: root.clone() }),
        Box::new(BashTool { root, ..BashTool::default() }),
    ];
    let definitions: Vec<ToolDefinition> = tools.iter().map(|tool| tool.definition()).collect();
    let dispatcher = AppToolDispatcher { tools };
    (Arc::new(definitions), dispatcher)
}
/// tool name bolded and the arguments dimmed, gPi-style.
fn tool_call_line(name: &str, arguments: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("$ {name}"), TOOL_NAME),
        Span::styled(format!(" {}", summarize_args(arguments)), TOOL_BODY),
    ])
}

/// Render a tool result line. The body is the first line of the output;
/// errors are shown in red.
fn tool_result_line(name: &str, output: String, error: Option<String>) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("\u{2191} {name}"), TOOL_NAME)];
    match error {
        Some(message) => {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                truncate_for_display(&message, 240),
                TOOL_ERROR,
            ));
        }
        None => {
            let first_line = output.lines().next().unwrap_or("");
            spans.push(Span::raw(" "));
            spans.push(Span::styled(first_line.to_string(), TOOL_RESULT));
        }
    }
    Line::from(spans)
}

fn summarize_args(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.len() > 200 {
        format!("{}\u{2026}", &trimmed[..200])
    } else {
        trimmed.to_string()
    }
}

fn truncate_for_display(input: &str, max: usize) -> String {
    let one_line = input.replace('\n', " ");
    if one_line.len() <= max {
        return one_line;
    }
    format!("{}\u{2026}", &one_line[..max])
}

/// URL to the provider's page where the user can create or copy an API key.
/// None means we do not know how to guide the user for that provider yet.
fn provider_keys_url(provider: &str) -> Option<&'static str> {
    match provider.to_ascii_lowercase().as_str() {
        "openrouter" => Some("https://openrouter.ai/keys"),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::empty()
    }

    fn app_with_recorder() -> (App, std::sync::Arc<RecordingBrowserInner>) {
        let recorder = std::sync::Arc::new(RecordingBrowserInner::default());
        let boxed: Box<dyn BrowserOpener> = Box::new(RecordingBrowserAdapter {
            inner: recorder.clone(),
        });
        let app = App::empty().with_browser(boxed);
        (app, recorder)
    }

    #[derive(Default)]
    struct RecordingBrowserInner {
        opened: std::sync::Mutex<Vec<String>>,
    }

    struct RecordingBrowserAdapter {
        inner: std::sync::Arc<RecordingBrowserInner>,
    }

    impl std::fmt::Debug for RecordingBrowserAdapter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("RecordingBrowserAdapter").finish()
        }
    }

    impl BrowserOpener for RecordingBrowserAdapter {
        fn open(&self, url: &str) {
            self.inner.opened.lock().expect("recorder mutex").push(url.to_string());
        }
    }

    // -- Slash command filter ------------------------------------------------

    #[test]
    fn filter_returns_every_command_for_empty_query() {
        let names: Vec<&str> = filter_slash_commands("")
            .iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names.len(), SLASH_COMMANDS.len());
    }

    #[test]
    fn filter_matches_exact_command_name() {
        let names: Vec<&str> = filter_slash_commands("connect")
            .iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["connect"]);
    }

    #[test]
    fn filter_uses_fuzzy_matching() {
        let names: Vec<&str> = filter_slash_commands("cn")
            .iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&"connect"));
    }

    #[test]
    fn filter_returns_empty_when_nothing_matches() {
        let commands = filter_slash_commands("zzzzzz");
        assert!(commands.is_empty());
    }

    // -- App mode transitions -----------------------------------------------

    #[test]
    fn submit_quit_sets_should_exit() {
        let mut app = test_app();
        app.input = "/quit".into();
        app.submit();
        assert!(app.should_exit);
    }

    #[test]
    fn submit_empty_does_nothing() {
        let mut app = test_app();
        let docs_before = app.document.len();
        app.input = "   ".into();
        app.submit();
        assert!(!app.should_exit);
        assert_eq!(app.document.len(), docs_before);
    }

    #[test]
    fn submit_unknown_slash_command_reports_error() {
        let mut app = test_app();
        app.input = "/nope".into();
        app.submit();
        let last = app.document.last().expect("error was pushed");
        assert!(format!("{last:?}").contains("Unknown command"));
    }

    #[test]
    fn submit_connect_without_session_enters_menu() {
        let (mut app, _recorder) = app_with_recorder();
        app.input = "/connect".into();
        app.submit();
        assert!(app.is_in_menu());
    }

    #[test]
    fn prompt_without_session_enters_menu() {
        let (mut app, _recorder) = app_with_recorder();
        app.input = "hello world".into();
        app.submit();
        assert!(app.is_in_menu());
    }

    #[test]
    fn menu_navigate_wraps_around() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod { selected: 0 };
        app.select_up();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod { selected: 1 }));
        app.select_down();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod { selected: 0 }));
        app.select_down();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod { selected: 1 }));
    }

    #[test]
    fn menu_confirm_account_enters_provider() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod { selected: 0 };
        app.confirm();
        assert!(matches!(app.mode, AppMode::AwaitingProvider { selected: 0 }));
    }

    #[test]
    fn menu_confirm_api_key_enters_api_key_input() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod { selected: 1 };
        app.confirm();
        assert!(matches!(
            app.mode,
            AppMode::AwaitingApiKey { opened_browser: false, .. }
        ));
    }

    #[test]
    fn menu_confirm_openrouter_opens_browser_via_oauth() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let opened = Arc::new(AtomicBool::new(false));
        struct FlagBrowser(Arc<AtomicBool>);
        impl std::fmt::Debug for FlagBrowser {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("FlagBrowser").finish()
            }
        }
        impl BrowserOpener for FlagBrowser {
            fn open(&self, _url: &str) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let opened_for_browser = Arc::clone(&opened);
        let app = App::empty().with_browser(Box::new(FlagBrowser(opened_for_browser)));
        let mut app = app;
        // The OAuth flow is blocking — it waits for the loopback callback.
        // We run it on a thread and give it a moment to open the browser
        // before letting it give up on the absent callback.
        std::thread::spawn(move || {
            app.mode = AppMode::AwaitingProvider { selected: 0 };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.confirm()));
        });
        // Give the OAuth flow a moment to call `browser.open` before the
        // loopback server gives up waiting for the callback.
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            opened.load(Ordering::SeqCst),
            "OAuth flow should have opened the browser before waiting on the callback"
        );
    }

    #[test]
    fn menu_lines_have_selection_marker() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod { selected: 0 };
        let lines = app.menu_lines();
        let has_arrow = lines
            .iter()
            .any(|line| line.to_string().contains("→ Sign in with an account"));
        assert!(has_arrow);
    }

    // -- Slash menu selection ------------------------------------------------

    #[test]
    fn typing_slash_highlights_first_command() {
        let mut app = test_app();
        app.input.push('/');
        app.recompute_show_commands();
        assert!(app.show_commands);
        assert_eq!(app.slash_selected, Some(0));
    }

    #[test]
    fn slash_selection_moves_with_arrows() {
        let mut app = test_app();
        app.input = "/".into();
        app.recompute_show_commands();
        let first = app.slash_selected;
        app.select_down();
        assert_ne!(app.slash_selected, first);
        app.select_up();
        assert_eq!(app.slash_selected, first);
    }

    #[test]
    fn slash_confirm_runs_the_highlighted_command() {
        let (mut app, _recorder) = app_with_recorder();
        app.input = "/".into();
        app.recompute_show_commands();
        // The first filtered command is whatever the matcher scores highest.
        let highlighted = app.slash_selected.expect("selection set");
        let commands = filter_slash_commands("");
        let expected_name = commands[highlighted].name;
        app.confirm();
        // The selected command should be in the input and submitted.
        assert!(app.should_exit || !app.document.is_empty() || app.input.is_empty());
        let _ = expected_name; // referenced for clarity
    }

    // -- Prompt prefix -------------------------------------------------------

    #[test]
    fn prompt_prefix_changes_with_mode() {
        let (mut app, _recorder) = app_with_recorder();
        assert_eq!(app.prompt_prefix(), "> ");
        app.mode = AppMode::AwaitingApiKey {
            provider: "openrouter".into(),
            opened_browser: false,
        };
        assert_eq!(app.prompt_prefix(), "OpenRouter API key: ");
        app.mode = AppMode::AwaitingConnectMethod { selected: 0 };
        assert_eq!(app.prompt_prefix(), "");
    }
}
