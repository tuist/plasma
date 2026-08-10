//! Headless Agent Client Protocol server.
//!
//! The Agent Client Protocol uses JSON-RPC over standard input and output for
//! local agents. Keep this module free of terminal UI concerns: an editor owns
//! the user interface while Plasma owns the inference session and tools.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, Implementation, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    SessionId, SessionNotification, SessionUpdate, StopReason,
};
use agent_client_protocol::{Agent, Result as AcpResult, Stdio};
use plasma_inference::ToolDefinition;
use plasma_openrouter::OpenRouterInferenceProvider;
use plasma_session::{AgentEvent, Session, ToolDispatcher};
use plasma_tools::{BashTool, ReadTool, Tool};
use serde_json::Value;

const AGENT_NAME: &str = "plasma";

type Sessions = Arc<Mutex<HashMap<SessionId, HeadlessSession>>>;

/// Run Plasma as a local [Agent Client Protocol](https://agentclientprotocol.com/)
/// agent. The protocol transport is exclusively standard input and output.
pub async fn run() -> AcpResult<()> {
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let next_session = Arc::new(Mutex::new(0_u64));
    let new_session_store = Arc::clone(&sessions);

    Agent
        .builder()
        .name(AGENT_NAME)
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new())
                        .agent_info(
                            Implementation::new(AGENT_NAME, env!("CARGO_PKG_VERSION"))
                                .title("Plasma"),
                        ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _connection| {
                let session_id = {
                    let mut next = next_session.lock().expect("session counter mutex poisoned");
                    *next += 1;
                    SessionId::new(format!("plasma-{}", *next))
                };
                let session = HeadlessSession::new(request.cwd);
                new_session_store
                    .lock()
                    .expect("session store mutex poisoned")
                    .insert(session_id.clone(), session);
                responder.respond(NewSessionResponse::new(session_id))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, connection| {
                let prompt = prompt_text(&request.prompt);
                let session_id = request.session_id.clone();
                let result = sessions
                    .lock()
                    .expect("session store mutex poisoned")
                    .get_mut(&session_id)
                    .map(|session| {
                        session.submit(prompt, |event| {
                            if let AgentEvent::Text(text) = event {
                                let _ = connection.send_notification(SessionNotification::new(
                                    session_id.clone(),
                                    SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                        text.into(),
                                    )),
                                ));
                            }
                        })
                    });

                match result {
                    Some(Ok(())) => responder.respond(PromptResponse::new(StopReason::EndTurn)),
                    Some(Err(error)) => {
                        connection.send_notification(SessionNotification::new(
                            session_id,
                            SessionUpdate::AgentMessageChunk(ContentChunk::new(error.into())),
                        ))?;
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    }
                    None => responder.respond(PromptResponse::new(StopReason::Refusal)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}

/// Per-ACP-session state. The editor supplies the working directory when it
/// creates the session, so local tools are always scoped to that workspace.
struct HeadlessSession {
    session: Result<Session<OpenRouterInferenceProvider>, String>,
    definitions: Arc<Vec<ToolDefinition>>,
    tools: LocalToolDispatcher,
}

impl HeadlessSession {
    fn new(root: PathBuf) -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ReadTool { root: root.clone() }),
            Box::new(BashTool {
                root,
                ..BashTool::default()
            }),
        ];
        let definitions = Arc::new(tools.iter().map(|tool| tool.definition()).collect());
        let session = OpenRouterInferenceProvider::from_saved_key()
            .map_err(|error| format!("Could not load the OpenRouter connection: {error}"))
            .and_then(|provider| {
                provider
                    .map(Session::new)
                    .ok_or_else(|| "Plasma is not connected. Run `plasma connect openrouter --api-key <key>` first.".to_string())
            });
        Self {
            session,
            definitions,
            tools: LocalToolDispatcher { tools },
        }
    }

    fn submit<F>(&mut self, prompt: String, mut on_event: F) -> Result<(), String>
    where
        F: FnMut(AgentEvent),
    {
        let session = self.session.as_mut().map_err(|error| error.clone())?;
        session
            .submit_with_tools(
                prompt,
                Arc::clone(&self.definitions),
                &self.tools,
                &mut |event| on_event(event),
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

struct LocalToolDispatcher {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolDispatcher for LocalToolDispatcher {
    fn dispatch(&self, name: &str, arguments_json: &str) -> Result<String, String> {
        let arguments: Value = serde_json::from_str(arguments_json)
            .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.name() == name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        tool.execute(&arguments)
    }
}

fn prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text(text) => text.text.clone(),
            ContentBlock::ResourceLink(link) => format!("[{}]({})", link.name, link.uri),
            _ => "[Unsupported prompt content omitted]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ResourceLink, TextContent};

    #[test]
    fn prompt_text_preserves_text_and_resource_links() {
        let prompt = prompt_text(&[
            ContentBlock::Text(TextContent::new("Inspect this:")),
            ContentBlock::ResourceLink(ResourceLink::new("readme", "file:///workspace/README.md")),
        ]);
        assert_eq!(
            prompt,
            "Inspect this:\n[readme](file:///workspace/README.md)"
        );
    }
}
