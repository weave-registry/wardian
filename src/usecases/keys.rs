//! Every secret on one list (ADR-2610081500). Each secret stays with the use case that uses it, a
//! `KeyOwner`; the keyring asks each owner what it holds, and passes on "test again" and "remove".
//! It also holds the admin token, and `KeyChecks`, the record of every test.

use crate::domain::package::unix_now;
use crate::ports::{secrets::Secrets, service::Keys, storage::FileSystem};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The shortest admin token Settings accepts.
const MIN_TOKEN_CHARS: usize = 24;

/// One secret as the list shows it. Never the secret itself.
pub struct KeyEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// "settings", "environment", or None when it is not set.
    pub from: Option<&'static str>,
    /// What it is, in a few words: "region us-east-1, API key", "token for https://splunk:8089".
    pub detail: String,
    /// Set when a saved secret is there but cannot be read.
    pub error: Option<String>,
}

/// A use case that holds secrets.
pub trait KeyOwner: Send + Sync {
    fn entries(&self) -> Vec<KeyEntry>;
    /// Tests the secret `id` again with its saved values, and records the result. None when `id`
    /// is not this owner's.
    fn retest(&self, id: &str) -> Option<Result<(), String>>;
    /// Removes the saved secret `id`; the environment's, if any, then applies. An error is about
    /// what comes after: the saved secret is already gone.
    fn forget(&self, id: &str) -> Option<Result<(), String>>;
}

/// The last test of each secret, in `key-checks.json`: {id: {at, ok, said}}. It holds no secret.
pub struct KeyChecks {
    fs: Arc<dyn FileSystem>,
    path: PathBuf,
    checks: Mutex<BTreeMap<String, Value>>,
    /// The file is there but cannot be read, and could not be moved aside: it is never written over.
    held: bool,
}

impl KeyChecks {
    /// A file that cannot be read is moved to `key-checks.json.unreadable`, so a new record never
    /// writes over the tests it holds.
    pub fn new(fs: Arc<dyn FileSystem>, data_dir: &Path) -> KeyChecks {
        let path = data_dir.join("key-checks.json");
        let read = fs.read(&path).and_then(|b| serde_json::from_slice(&b).ok());
        let mut held = false;
        if read.is_none() && fs.exists(&path) {
            let aside = data_dir.join("key-checks.json.unreadable");
            match fs.rename(&path, &aside) {
                Ok(()) => eprintln!("keys: {} cannot be read; moved it to {}", path.display(), aside.display()),
                Err(e) => {
                    eprintln!("keys: {} cannot be read, or moved aside ({e}); tests will not be saved", path.display());
                    held = true;
                }
            }
        }
        KeyChecks { fs, path, checks: Mutex::new(read.unwrap_or_default()), held }
    }

    /// Records a test of `id`: when, whether it passed, and what the service said.
    pub fn record<T>(&self, id: &str, outcome: &Result<T, String>) {
        let said = match outcome {
            Ok(_) => "accepted".to_string(),
            Err(e) => e.clone(),
        };
        let mut checks = self.checks.lock().unwrap();
        checks.insert(id.into(), json!({ "at": unix_now(), "ok": outcome.is_ok(), "said": said }));
        self.save(&checks);
    }

    pub fn forget(&self, id: &str) {
        let mut checks = self.checks.lock().unwrap();
        if checks.remove(id).is_some() {
            self.save(&checks);
        }
    }

    pub fn last(&self, id: &str) -> Value {
        self.checks.lock().unwrap().get(id).cloned().unwrap_or(Value::Null)
    }

    fn save(&self, checks: &BTreeMap<String, Value>) {
        if self.held {
            return;
        }
        let bytes = serde_json::to_vec_pretty(checks).unwrap_or_default();
        if let Err(e) = self.fs.write_private(&self.path, &bytes) {
            eprintln!("keys: could not save {}: {e}", self.path.display());
        }
    }
}

/// The admin token: ADMIN_TOKEN, or the one saved in Settings (`admin-token`). The environment wins.
pub struct AdminGate {
    secrets: Arc<dyn Secrets>,
    path: PathBuf,
    env: Option<String>,
    saved: Mutex<Option<String>>,
    /// Wardian listens on an address other machines can reach.
    public: bool,
}

impl AdminGate {
    /// A saved token that cannot be read is an error, so Wardian never starts with its settings
    /// open because a lock it was given could not be read.
    pub fn load(secrets: Arc<dyn Secrets>, data_dir: &Path, env: Option<String>, public: bool) -> Result<AdminGate, String> {
        let path = data_dir.join("admin-token");
        let saved = secrets
            .read(&path)
            .map_err(|e| format!("the admin token in {} cannot be read ({e}). Remove that file to start without it.", path.display()))?
            .map(|b| String::from_utf8_lossy(&b).trim().to_string())
            .filter(|t| !t.is_empty());
        Ok(AdminGate { secrets, path, env, saved: Mutex::new(saved), public })
    }

    pub fn token(&self) -> Option<String> {
        self.env.clone().or_else(|| self.saved.lock().unwrap().clone())
    }

    fn entry(&self) -> KeyEntry {
        let from = if self.env.is_some() {
            Some("environment")
        } else if self.saved.lock().unwrap().is_some() {
            Some("settings")
        } else {
            None
        };
        let detail = match from {
            Some(_) => "every admin request must send it".to_string(),
            None if self.public => "not set".to_string(),
            None => "not set: every program on this machine is an admin".to_string(),
        };
        KeyEntry { id: "admin", name: "Admin token", from, detail, error: None }
    }

    /// `{token}` saves that token; `{generate: true}` makes one and returns it, once.
    fn set(&self, body: &Value) -> Result<Value, String> {
        if self.env.is_some() {
            return Err("ADMIN_TOKEN is set in the environment, and it wins: change it there".into());
        }
        let generated = body["generate"].as_bool() == Some(true);
        let token = if generated { self.secrets.new_token() } else { body["token"].as_str().unwrap_or("").trim().to_string() };
        if token.chars().count() < MIN_TOKEN_CHARS || token.len() > 200 || !token.chars().all(|c| c.is_ascii_graphic()) {
            return Err(format!("the admin token must be {MIN_TOKEN_CHARS} to 200 printable characters, with no spaces"));
        }
        // Held across the write, so the token in memory is always the one in the file.
        let mut saved = self.saved.lock().unwrap();
        self.secrets.write(&self.path, token.as_bytes()).map_err(|e| format!("saving the admin token: {e}"))?;
        *saved = Some(token.clone());
        // The browser that set it keeps it for its tab; a made one is shown this once.
        Ok(json!({ "set": true, "token": token, "generated": generated }))
    }

    fn forget(&self) -> Result<(), String> {
        if self.env.is_some() {
            return Err("ADMIN_TOKEN is set in the environment: unset it there".into());
        }
        if self.public {
            return Err("Wardian listens on an address other machines can reach, so it keeps its admin token. Replace it instead.".into());
        }
        let mut saved = self.saved.lock().unwrap();
        self.secrets.remove(&self.path);
        // A file the disk would not remove is the token again after a restart, so it stays in force.
        if !matches!(self.secrets.read(&self.path), Ok(None)) {
            return Err(format!("the admin token in {} could not be removed: check that Wardian can write to its data folder", self.path.display()));
        }
        *saved = None;
        Ok(())
    }
}

pub struct Keyring {
    owners: Vec<Arc<dyn KeyOwner>>,
    admin: Arc<AdminGate>,
    checks: Arc<KeyChecks>,
    secrets: Arc<dyn Secrets>,
    /// Held by a test and by a remove, so a test never records its result after its key is removed.
    testing: Mutex<()>,
}

impl Keyring {
    pub fn new(owners: Vec<Arc<dyn KeyOwner>>, admin: Arc<AdminGate>, checks: Arc<KeyChecks>, secrets: Arc<dyn Secrets>) -> Keyring {
        Keyring { owners, admin, checks, secrets, testing: Mutex::new(()) }
    }

    fn entry_json(&self, e: &KeyEntry) -> Value {
        // On a public address the admin token is kept: `AdminGate::forget` refuses to remove it.
        let removable = e.from == Some("settings") && !(e.id == "admin" && self.admin.public);
        json!({ "id": e.id, "name": e.name, "set": e.from.is_some(), "from": e.from, "detail": e.detail,
                "error": e.error, "check": self.checks.last(e.id), "removable": removable,
                "testable": e.id != "admin" && e.from.is_some() })
    }
}

impl Keys for Keyring {
    fn list(&self) -> Value {
        let mut keys: Vec<Value> = self.owners.iter().flat_map(|o| o.entries()).map(|e| self.entry_json(&e)).collect();
        keys.push(self.entry_json(&self.admin.entry()));
        json!({ "keys": keys, "kept": self.secrets.describe() })
    }

    fn test(&self, id: &str) -> Result<Value, String> {
        if id == "admin" {
            return Err("the admin token has no service to test it against".into());
        }
        let testing = self.testing.lock().unwrap();
        let outcome = self.owners.iter().find_map(|o| o.retest(id)).ok_or_else(|| format!("no key \"{id}\""))?;
        drop(testing);
        let mut out = self.list();
        out["tested"] = json!({ "id": id, "ok": outcome.is_ok(), "said": outcome.err() });
        Ok(out)
    }

    fn remove(&self, id: &str) -> Result<Value, String> {
        if id == "admin" {
            self.admin.forget()?;
            self.checks.forget(id);
        } else {
            // An environment key has nothing saved to remove, and its last test still applies.
            if self.owners.iter().flat_map(|o| o.entries()).any(|e| e.id == id && e.from == Some("environment")) {
                return Err(format!("the key \"{id}\" is set in the environment: unset it there"));
            }
            let _testing = self.testing.lock().unwrap();
            let forgot = self.owners.iter().find_map(|o| o.forget(id)).ok_or_else(|| format!("no key \"{id}\""))?;
            // An owner removes the saved secret before what can fail after it, so its test goes either way.
            self.checks.forget(id);
            forgot?;
        }
        Ok(self.list())
    }

    fn set_admin_token(&self, body: &Value) -> Result<Value, String> {
        let mut out = self.admin.set(body)?;
        out["keys"] = self.list()["keys"].clone();
        Ok(out)
    }

    fn admin_token(&self) -> Option<String> {
        self.admin.token()
    }
}
