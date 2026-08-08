use std::{fs, io, path::PathBuf};

use directories::ProjectDirs;
use plasma_inference::{InferenceError, InferenceProvider, ToolDefinition};
use plasma_protocol::{Message, Role, ToolCall};
use serde_json::{Value, json};

const DEFAULT_MODEL: &str = "openai/gpt-4.1-mini";

pub struct OpenRouterInferenceProvider {
    api_key: String,
    model: String,
}

impl OpenRouterInferenceProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
        }
    }

    pub fn from_saved_key() -> io::Result<Option<Self>> {
        load_key().map(|key| key.map(Self::new))
    }
}

impl InferenceProvider for OpenRouterInferenceProvider {
    fn name(&self) -> &str {
        "OpenRouter"
    }

    fn complete(
        &mut self,
        history: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Message, InferenceError> {
        let messages: Vec<Value> = history
            .iter()
            .map(message_to_openai)
            .collect();
        let tools_payload: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect();
        let mut body = json!({"model": self.model, "messages": messages});
        if !tools_payload.is_empty() {
            body["tools"] = Value::Array(tools_payload);
        }
        let response = ureq::post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/tuist/plasma")
            .send_json(body)
            .map_err(|error| InferenceError::Request(error.to_string()))?;
        let mut response_body = response.into_body();
        let value: Value = serde_json::from_str(
            &response_body
                .read_to_string()
                .map_err(|error| InferenceError::Request(error.to_string()))?,
        )
        .map_err(|error| InferenceError::Request(error.to_string()))?;
        let message_value = &value["choices"][0]["message"];
        if let Some(calls) = message_value.get("tool_calls").and_then(Value::as_array) {
            let tool_calls: Vec<ToolCall> = calls
                .iter()
                .map(parse_tool_call)
                .collect::<Result<_, _>>()?;
            let content = message_value
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            return Ok(Message::assistant_with_tool_calls(content, tool_calls));
        }
        let content = message_value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                InferenceError::Request("response did not include assistant content".into())
            })?;
        Ok(Message::assistant(content))
    }
}

fn message_to_openai(message: &Message) -> Value {
    let mut object = json!({"role": role_name(&message.role), "content": message.content});
    if let Some(calls) = &message.tool_calls {
        let calls: Vec<Value> = calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": call.arguments,
                    }
                })
            })
            .collect();
        object["tool_calls"] = Value::Array(calls);
    }
    if let Some(id) = &message.tool_call_id {
        object["tool_call_id"] = Value::String(id.clone());
    }
    object
}

fn parse_tool_call(value: &Value) -> Result<ToolCall, InferenceError> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| InferenceError::Request("tool call missing id".into()))?
        .to_string();
    let function = value
        .get("function")
        .ok_or_else(|| InferenceError::Request("tool call missing function".into()))?;
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| InferenceError::Request("tool call missing name".into()))?
        .to_string();
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}")
        .to_string();
    Ok(ToolCall {
        id,
        name,
        arguments,
    })
}

pub fn save_key(api_key: &str) -> io::Result<()> {
    let path = key_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, api_key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn load_key() -> io::Result<Option<String>> {
    match fs::read_to_string(key_path()?) {
        Ok(key) => Ok(Some(key)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn key_path() -> io::Result<PathBuf> {
    ProjectDirs::from("dev", "tuist", "plasma")
        .map(|directories| directories.config_dir().join("openrouter.key"))
        .ok_or_else(|| io::Error::other("could not resolve a configuration directory"))
}

fn role_name(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}
