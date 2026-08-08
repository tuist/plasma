use std::{fs, io, path::PathBuf};

use directories::ProjectDirs;
use plasma_inference::{InferenceError, InferenceProvider};
use plasma_protocol::{Message, Role};
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

    fn complete(&mut self, history: &[Message]) -> Result<Message, InferenceError> {
        let messages: Vec<Value> = history
            .iter()
            .map(|message| json!({"role": role_name(&message.role), "content": message.content}))
            .collect();
        let response = ureq::post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/tuist/plasma")
            .send_json(json!({"model": self.model, "messages": messages}))
            .map_err(|error| InferenceError::Request(error.to_string()))?;
        let mut body = response.into_body();
        let value: Value = serde_json::from_str(
            &body
                .read_to_string()
                .map_err(|error| InferenceError::Request(error.to_string()))?,
        )
        .map_err(|error| InferenceError::Request(error.to_string()))?;
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                InferenceError::Request("response did not include assistant content".into())
            })?;
        Ok(Message::assistant(content))
    }
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
