//! Claude, through the Anthropic API or Amazon Bedrock (ADR-2610071106). The adapters know the
//! model names and the wire formats; callers send a Messages body and ask for a tier.

use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Tier {
    /// The model that builds apps, and answers `claude:sample` by default.
    Main,
    /// A fast model for `{modelTier: 'quick'}` requests.
    Quick,
}

/// How Bedrock is signed in to.
#[derive(Clone)]
pub enum BedrockAuth {
    /// A Bedrock API key, sent as a bearer token.
    ApiKey(String),
    /// AWS access keys, signed with Signature Version 4; `session` is empty without a session token.
    AccessKeys { id: String, secret: String, session: String },
}

/// The credentials of one provider.
#[derive(Clone)]
pub enum LlmAuth {
    /// The key, and the workspace for keys that are not scoped to one.
    Anthropic { key: String, workspace: String },
    Bedrock { region: String, auth: BedrockAuth },
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
    /// One Messages request; `body` is complete, model included. Each adapter maps it to its API.
    fn messages(&self, auth: &LlmAuth, body: &Value) -> Result<Value, LlmError>;
    /// A cheap request that proves the credentials work, and that they may use `model`.
    fn test_key(&self, auth: &LlmAuth, model: &str) -> Result<(), LlmError>;
}
