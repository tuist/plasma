//! Non-interactive execution for scripts, continuous integration, and pipes.

use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::agent_prompt::CODING_AGENT_PROMPT;
use crate::cli::ColorChoice;
use crate::syntax::highlight_markdown;
use anyhow::{Context, Result, anyhow};
use plasma_inference::ToolDefinition;
use plasma_openrouter::OpenRouterInferenceProvider;
use plasma_session::{AgentEvent, Session, ToolDispatcher};
use plasma_tools::{BashTool, ReadTool, Tool};
use serde_json::{Value, json};

/// Execute a single task and write only the final response to standard output.
pub fn run(
    prompt: Option<String>,
    api_key: Option<String>,
    cwd: Option<PathBuf>,
    verbose: bool,
    json_output: bool,
    color: ColorChoice,
) -> Result<()> {
    let prompt = prompt.map(Ok).unwrap_or_else(read_prompt)?;
    anyhow::ensure!(!prompt.trim().is_empty(), "a prompt is required");

    let root = cwd.unwrap_or(std::env::current_dir()?);
    anyhow::ensure!(
        root.is_dir(),
        "workspace does not exist: {}",
        root.display()
    );
    let api_key = api_key.or_else(|| std::env::var("PLASMA_OPENROUTER_API_KEY").ok());
    let provider = match api_key {
        Some(key) => OpenRouterInferenceProvider::new(key),
        None => OpenRouterInferenceProvider::from_saved_key()?
            .ok_or_else(|| anyhow!("not connected; run `plasma connect openrouter --api-key <key>` or set PLASMA_OPENROUTER_API_KEY"))?,
    };
    let (definitions, dispatcher) = build_tool_chain(root);
    let mut session = Session::with_system_prompt(provider, CODING_AGENT_PROMPT);
    let response = session
        .submit_with_tools(prompt, definitions, &dispatcher, &mut |event| {
            if verbose {
                print_event(event);
            }
        })
        .map_err(|error| anyhow!(error.to_string()))?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string(&json!({ "response": response.content }))?
        );
    } else {
        let output = if matches!(color, ColorChoice::Always)
            || (matches!(color, ColorChoice::Auto) && io::stdout().is_terminal())
        {
            highlight_markdown(&response.content)
        } else {
            response.content
        };
        println!("{output}");
    }
    Ok(())
}

fn read_prompt() -> Result<String> {
    anyhow::ensure!(
        !io::stdin().is_terminal(),
        "a prompt is required; use `plasma exec <prompt>`, `plasma -p <prompt>`, or pipe it on standard input"
    );
    let mut prompt = String::new();
    io::stdin()
        .read_to_string(&mut prompt)
        .context("could not read prompt from standard input")?;
    Ok(prompt)
}

fn print_event(event: AgentEvent) {
    match event {
        AgentEvent::Text(text) => eprintln!("assistant: {text}"),
        AgentEvent::ToolCall { name, arguments } => eprintln!("tool call: {name} {arguments}"),
        AgentEvent::ToolResult {
            name,
            output,
            error,
        } => match error {
            Some(error) => eprintln!("tool error: {name}: {error}"),
            None => eprintln!("tool result: {name}: {output}"),
        },
    }
}

struct HeadlessToolDispatcher {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolDispatcher for HeadlessToolDispatcher {
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

fn build_tool_chain(root: PathBuf) -> (Arc<Vec<ToolDefinition>>, HeadlessToolDispatcher) {
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(ReadTool { root: root.clone() }),
        Box::new(BashTool {
            root,
            timeout: Duration::from_secs(30),
        }),
    ];
    let definitions = tools.iter().map(|tool| tool.definition()).collect();
    (Arc::new(definitions), HeadlessToolDispatcher { tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_chain_exposes_read_and_bash() {
        let (definitions, _) = build_tool_chain(std::env::temp_dir());
        let names: Vec<_> = definitions.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names, ["read", "bash"]);
    }
}
