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
    /// An AWS profile (ADR-2610091530): its name, and the AWS CLI found when it was saved (empty
    /// without one). The keys come from [`AwsCredentials`] when a request needs them.
    Profile { name: String, cli: String },
}

/// AWS access keys for one request: the ID, the secret, and a session token (empty without one).
pub struct AwsKeys {
    pub id: String,
    pub secret: String,
    pub session: String,
}

/// Never prints a key.
impl std::fmt::Debug for AwsKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AwsKeys")
    }
}

/// Where the keys of an AWS profile come from (ADR-2610091530): the AWS CLI, the profile's
/// `credential_process`, or the profile's files. Kept in memory only, until shortly before they
/// expire. An error is a sentence for Settings, and never holds a key.
pub trait AwsCredentials: Send + Sync {
    fn keys(&self, profile: &str, cli: &str) -> Result<AwsKeys, String>;
    /// Drops the keys kept for `profile`, so the next request fetches new ones (after AWS refused them).
    fn forget(&self, profile: &str);
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
    /// No keys to sign in with (an AWS profile that could not give any): a sentence for Settings.
    SignIn(String),
}

pub trait Llm: Send + Sync {
    fn model(&self, tier: Tier) -> String;
    /// One Messages request; `body` is complete, model included. Each adapter maps it to its API.
    fn messages(&self, auth: &LlmAuth, body: &Value) -> Result<Value, LlmError>;
    /// A cheap request that proves the credentials work, and that they may use `model`.
    fn test_key(&self, auth: &LlmAuth, model: &str) -> Result<(), LlmError>;
}
