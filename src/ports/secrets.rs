//! Where secrets are kept (ADR-2610081500): API keys, the Splunk account, the Google service
//! account key and the admin token. Every use case that holds one reads and writes it here, never
//! through `FileSystem`, so how they are kept is decided in one adapter.

use serde_json::Value;
use std::path::Path;

pub trait Secrets: Send + Sync {
    /// The secret at `path`: None when there is none, an error when one is there but cannot be
    /// read, so Settings can say so instead of showing "not set".
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, String>;
    /// Writes the secret readable by its owner only, through a temp file and a rename.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
    fn remove(&self, path: &Path);
    /// A new random token for the user to keep: 32 bytes from the system's secure random source,
    /// as 64 hex digits.
    fn new_token(&self) -> String;
    /// How secrets are kept, for Settings: {sealed, master?, note}. Never a secret.
    fn describe(&self) -> Value;
}
