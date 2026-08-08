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

use crate::browser::{BrowserOpener, SystemBrowser};
use crate::theme::{SLASH_COMMAND, SUCCESS, TITLE};

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

/// The state machine the TUI walks through when the user has no session.
#[derive(Clone)]
pub enum AppMode {
    Normal,
    /// Asking the user whether they want to connect an account or paste a key.
    AwaitingConnectMethod,
    /// Asking the user which provider to connect.
    AwaitingProvider,
    /// Awaiting the user to type the API key. `opened_browser` records
    /// whether the host browser was opened for this attempt, so the prompt
    /// can mention it.
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
    pub should_exit: bool,
    browser: Box<dyn BrowserOpener>,
}

impl App {
    /// Create the app used by the TUI on launch. Reads any saved OpenRouter
    /// key from disk and seeds the welcome document.
    pub fn new() -> Self {
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
            app.document.push(Line::from(
                "Type /connect to connect your account, or a prompt to start.",
            ));
        }
        app
    }

    /// Create a bare app with no filesystem access. Used by tests so they
    /// can run in parallel without touching the user's config.
    pub fn empty() -> Self {
        Self {
            session: None,
            input: String::new(),
            document: Vec::new(),
            mode: AppMode::Normal,
            show_commands: false,
            should_exit: false,
            browser: Box::new(SystemBrowser),
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
            AppMode::Normal
            | AppMode::AwaitingConnectMethod
            | AppMode::AwaitingProvider => "> ",
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
        filter_slash_commands(query)
            .into_iter()
            .map(|command| {
                Line::from(format!("/{:<10} {}", command.name, command.description))
                    .style(Style::default())
                    .patch_style(SLASH_COMMAND)
            })
            .collect()
    }

    pub fn should_show_commands(&self) -> bool {
        self.show_commands && self.is_normal()
    }

    /// Recompute `show_commands` from the current input. Called by the input
    /// handler after every change so the menu hides as soon as the typed
    /// query no longer matches any slash command.
    pub fn recompute_show_commands(&mut self) {
        self.show_commands = self.is_normal()
            && self.input.starts_with('/')
            && !filter_slash_commands(self.input.trim_start_matches('/')).is_empty();
    }

    pub fn push_prompt_echo(&mut self, prompt: &str) {
        self.document.push(Line::from(format!("> {prompt}")));
    }

    pub fn push_info(&mut self, message: impl Into<String>) {
        self.document.push(Line::from(message.into()));
    }

    /// Handle the user pressing Enter. Dispatches to the right handler based
    /// on the current mode.
    pub fn submit(&mut self) {
        let input = std::mem::take(&mut self.input);
        self.show_commands = false;
        match self.mode.clone() {
            AppMode::Normal => self.submit_normal(&input),
            AppMode::AwaitingConnectMethod => self.submit_connect_method(&input),
            AppMode::AwaitingProvider => self.submit_provider(&input),
            AppMode::AwaitingApiKey { provider, opened_browser } => {
                self.submit_api_key(&provider, opened_browser, &input);
            }
        }
    }

    fn submit_normal(&mut self, input: &str) {
        match input.trim() {
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

    fn submit_connect_method(&mut self, input: &str) {
        match input.trim().to_lowercase().as_str() {
            "account" => self.enter_provider(),
            "api-key" | "apikey" | "key" => self.begin_api_key_input("openrouter", false),
            _ => self.push_info(format!(
                "Unknown option '{input}'. Type 'account' or 'api-key'."
            )),
        }
    }

    fn submit_provider(&mut self, input: &str) {
        match input.trim().to_lowercase().as_str() {
            "openrouter" => self.begin_api_key_input("openrouter", true),
            _ => self.push_info(format!("Unknown provider '{input}'. Type 'openrouter'.")),
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
        self.document.push(Line::from("How would you like to connect?"));
        self.document.push(Line::from(""));
        self.document
            .push(Line::from("  account      Sign in via your browser"));
        self.document
            .push(Line::from("  api-key      Paste an API key directly"));
        self.mode = AppMode::AwaitingConnectMethod;
    }

    fn enter_provider(&mut self) {
        self.document.push(Line::from(""));
        self.document.push(Line::from("Choose a provider:"));
        self.document.push(Line::from(""));
        self.document
            .push(Line::from("  openrouter   OpenRouter"));
        self.mode = AppMode::AwaitingProvider;
    }

    /// Move into the API-key input mode. If `opened_browser` is true, the
    /// host browser is opened to the provider's key page so the user can
    /// create or copy a key without leaving the agent.
    fn begin_api_key_input(&mut self, provider: &str, opened_browser: bool) {
        if opened_browser {
            if let Some(url) = provider_keys_url(provider) {
                self.browser.open(url);
                self.document.push(Line::from(""));
                self.document.push(Line::from(format!(
                    "Opening {url} in your browser. Create or copy a key there, then paste it below."
                )));
            }
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
        match self.session.as_mut() {
            Some(session) => match session.submit(prompt) {
                Ok(message) => self.document.push(Line::from(message.content)),
                Err(error) => self.push_info(format!("Request failed: {error}")),
            },
            None => unreachable!("send_prompt is only called when a session exists"),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::empty()
    }
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

    // Helper that builds an app with a recording browser and returns both.
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
        // "cn" should still match "connect" via fuzzy matching.
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
    fn submit_connect_without_session_enters_connect_method() {
        let (mut app, _recorder) = app_with_recorder();
        app.input = "/connect".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod));
    }

    #[test]
    fn prompt_without_session_enters_connect_method() {
        let (mut app, _recorder) = app_with_recorder();
        app.input = "hello world".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod));
        // The original prompt is dropped so the user is not surprised by a
        // "Send hello world to the model?" echo after they finish connecting.
    }

    #[test]
    fn connect_method_account_enters_provider() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod;
        app.input = "account".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingProvider));
    }

    #[test]
    fn connect_method_account_is_case_insensitive() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod;
        app.input = "ACCOUNT".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingProvider));
    }

    #[test]
    fn connect_method_api_key_aliases_work() {
        for alias in ["api-key", "apikey", "key"] {
            let (mut app, _recorder) = app_with_recorder();
            app.mode = AppMode::AwaitingConnectMethod;
            app.input = alias.into();
            app.submit();
            assert!(
                matches!(app.mode, AppMode::AwaitingApiKey { opened_browser: false, .. }),
                "alias {alias} did not enter AwaitingApiKey"
            );
        }
    }

    #[test]
    fn connect_method_unknown_option_keeps_mode() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingConnectMethod;
        app.input = "nope".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingConnectMethod));
        // The error is pushed to the document.
        let last = app.document.last().expect("an error was pushed");
        assert!(format!("{last:?}").contains("Unknown option"));
    }

    #[test]
    fn provider_openrouter_opens_browser_and_enters_api_key() {
        let (mut app, recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingProvider;
        app.input = "openrouter".into();
        app.submit();
        assert!(matches!(
            app.mode,
            AppMode::AwaitingApiKey { opened_browser: true, .. }
        ));
        let opened = recorder.opened.lock().unwrap();
        assert_eq!(opened.as_slice(), &["https://openrouter.ai/keys".to_string()]);
    }

    #[test]
    fn provider_unknown_keeps_mode() {
        let (mut app, _recorder) = app_with_recorder();
        app.mode = AppMode::AwaitingProvider;
        app.input = "anthropic".into();
        app.submit();
        assert!(matches!(app.mode, AppMode::AwaitingProvider));
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
    }
}
