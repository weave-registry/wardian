//! The meter (ADR-2610081500): adds each Claude reply's tokens to `usage.json`, and says how much
//! a payer used today, for the caps.

use crate::domain::package::unix_now;
use crate::domain::usage::{day_of, Tokens, UsageBook};
use crate::ports::storage::FileSystem;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct Meter {
    fs: Arc<dyn FileSystem>,
    path: PathBuf,
    book: Mutex<UsageBook>,
}

impl Meter {
    pub fn new(fs: Arc<dyn FileSystem>, data_dir: &Path) -> Meter {
        let path = data_dir.join("usage.json");
        let book = fs.read(&path).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Meter { fs, path, book: Mutex::new(book) }
    }

    /// Adds one reply's `usage` for `payer`. A failed save is reported and the count kept in memory.
    pub fn record(&self, payer: &str, reply: &Value) {
        let mut book = self.book.lock().unwrap();
        book.add(&day_of(unix_now()), payer, &Tokens::of_reply(reply));
        match serde_json::to_vec(&*book) {
            Ok(bytes) => {
                if let Err(e) = self.fs.write_private(&self.path, &bytes) {
                    eprintln!("usage: could not save {}: {e}", self.path.display());
                }
            }
            Err(e) => eprintln!("usage: {e}"),
        }
    }

    /// The tokens `payer` used today (UTC), as a cap counts them.
    pub fn used_today(&self, payer: &str) -> u64 {
        self.book.lock().unwrap().used(&day_of(unix_now()), payer)
    }

    pub fn report(&self) -> Value {
        self.book.lock().unwrap().report(&day_of(unix_now()))
    }
}
