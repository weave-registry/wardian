//! Secrets as plain files readable by their owner only (mode 600), in the data folder.

use crate::ports::{secrets::Secrets, storage::FileSystem};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

pub struct PlainSecrets {
    fs: Arc<dyn FileSystem>,
}

impl PlainSecrets {
    pub fn new(fs: Arc<dyn FileSystem>) -> PlainSecrets {
        PlainSecrets { fs }
    }
}

impl Secrets for PlainSecrets {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        match self.fs.read(path) {
            Some(bytes) => Ok(Some(bytes)),
            None if self.fs.exists(path) => Err("the file is there, but it cannot be opened".into()),
            None => Ok(None),
        }
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.fs.write_private(path, bytes)
    }
    fn remove(&self, path: &Path) {
        self.fs.remove_file(path)
    }
    fn new_token(&self) -> String {
        new_token()
    }
    fn describe(&self) -> Value {
        json!({ "sealed": false, "note": "in files only their owner can read" })
    }
}

/// 32 bytes from the system's secure random source, as 64 hex digits.
pub fn new_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).expect("the system's random source failed");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_new_tokens_are_long_and_differ() {
        let (a, b) = (super::new_token(), super::new_token());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
