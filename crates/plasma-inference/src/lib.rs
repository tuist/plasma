use plasma_protocol::Message;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("the provider is not connected")]
    NotConnected,
    #[error("provider request failed: {0}")]
    Request(String),
}

pub trait InferenceProvider {
    fn name(&self) -> &str;
    fn complete(&mut self, history: &[Message]) -> Result<Message, InferenceError>;
}
