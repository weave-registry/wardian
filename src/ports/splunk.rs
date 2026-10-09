//! Splunk's REST API (the management port, usually 8089).

pub use crate::domain::splunk::{cert_names, CertNames, SplunkConfig};
use serde_json::Value;

/// A failed call. Each text says what was asked, what came back, and the likely cause. The kinds
/// let setup tell Splunk's API from other things on the same host (ADR-2610091500).
pub enum SplunkError {
    /// Something else went wrong, such as a search Splunk refused.
    Call(String),
    /// Splunk answered, but not with JSON.
    Unreadable { url: String, reason: String },
    /// Something answered, but it is not Splunk's API: a web page, a redirect, a 404.
    NotTheApi(String),
    /// Nothing usable answered: the connection was refused, closed, timed out, or did not speak TLS.
    Unreachable(String),
    /// Splunk's API answered and refused the account (401 or 403).
    Refused(String),
    /// The server's certificate is not trusted; `names` says whose it is, when it could be read.
    Certificate { text: String, names: Option<CertNames> },
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
