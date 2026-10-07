//! Splunk's REST API (the management port, usually 8089).

pub use crate::domain::splunk::SplunkConfig;
use serde_json::Value;

pub enum SplunkError {
    /// The call failed; the text says what was asked, what came back, and the likely cause.
    Call(String),
    /// Splunk answered, but not with JSON.
    Unreadable { url: String, reason: String },
}

pub trait SplunkApi: Send + Sync {
    /// A connection with this account's address, credentials and certificate settings.
    fn connect(&self, cfg: &SplunkConfig) -> Result<Box<dyn SplunkSession>, String>;
}

/// Calls under the account's address; `path` starts after it, like "/services/search/jobs".
pub trait SplunkSession {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, SplunkError>;
    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<Value, SplunkError>;
    /// The full address of `path`, for messages.
    fn url(&self, path: &str) -> String;
}
