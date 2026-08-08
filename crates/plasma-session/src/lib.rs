use plasma_inference::{InferenceError, InferenceProvider};
use plasma_protocol::Message;

pub struct Session<P> {
    provider: P,
    history: Vec<Message>,
}

impl<P: InferenceProvider> Session<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            history: Vec::new(),
        }
    }
    pub fn history(&self) -> &[Message] {
        &self.history
    }
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn submit(&mut self, prompt: impl Into<String>) -> Result<Message, InferenceError> {
        self.history.push(Message::user(prompt));
        let response = self.provider.complete(&self.history)?;
        self.history.push(response.clone());
        Ok(response)
    }
}
