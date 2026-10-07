//! Claude, through the Anthropic API. The adapter knows the model names; callers ask for a tier.

use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Tier {
    /// The model that builds apps, and answers `claude:sample` by default.
    Main,
    /// A fast model for `{modelTier: 'quick'}` requests.
    Quick,
}

/// The key, and the workspace for keys that are not scoped to one.
#[derive(Clone)]
pub struct LlmAuth {
    pub key: String,
    pub workspace: String,
}

pub enum LlmError {
    /// The API answered with an HTTP error: the code and its own message, if any.
    Status(u16, String),
    /// The API could not be reached.
    Transport(String),
    /// The reply could not be read.
    Unreadable(String),
}

pub trait Llm: Send + Sync {
    fn model(&self, tier: Tier) -> String;
    /// One request to /v1/messages; `body` is complete, model included.
    fn messages(&self, auth: &LlmAuth, body: &Value) -> Result<Value, LlmError>;
    /// Asks for the main model's details: a cheap way to test a key.
    fn test_key(&self, auth: &LlmAuth) -> Result<(), LlmError>;
}
