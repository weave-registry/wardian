//! Secrets sealed with AES-256-GCM under a master key kept outside the data folder
//! (ADR-2610081501), so a copy of the data folder holds no readable secret.
//!
//! A sealed file is `HEADER`, a 12-byte random nonce, and the ciphertext with its tag; the file's
//! name is the additional data, so a sealed file put under another name does not open. A secret
//! written before sealing has no header: it is read as it is and sealed in place.

use crate::ports::{
    secrets::{MasterKey, Secrets},
    storage::FileSystem,
};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN},
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const HEADER: &[u8] = b"WARDIAN-SEALED-1\n";

/// A place that can keep the master key: a variable or a key file.
pub trait KeyPlace: Send + Sync {
    /// How Settings and the start lines name it.
    fn name(&self) -> String;
    /// Some key when it holds one, None when it holds none, an error when it cannot be asked.
    fn get(&self) -> Result<Option<[u8; 32]>, String>;
    /// Keeps a new key here.
    fn put(&self, key: &[u8; 32]) -> Result<(), String>;
}

fn hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    let t = text.trim();
    if t.len() != 64 || !t.is_ascii() {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    SystemRandom::new().fill(&mut bytes).expect("the system's random source failed");
    bytes
}

/// 32 bytes from the system's secure random source, as 64 hex digits.
pub fn new_token() -> String {
    hex(&random::<32>())
}

/// `WARDIAN_MASTER_KEY`: given, never written.
pub struct EnvKey(pub String);

impl KeyPlace for EnvKey {
    fn name(&self) -> String {
        "WARDIAN_MASTER_KEY".into()
    }
    fn get(&self) -> Result<Option<[u8; 32]>, String> {
        unhex(&self.0).map(Some).ok_or_else(|| "WARDIAN_MASTER_KEY must be 64 hex digits".into())
    }
    fn put(&self, _: &[u8; 32]) -> Result<(), String> {
        Err("WARDIAN_MASTER_KEY is read, never written".into())
    }
}

/// A file holding the key as 64 hex digits: `WARDIAN_MASTER_KEY_FILE`, or the one in the user's folder.
pub struct KeyFile {
    pub fs: Arc<dyn FileSystem>,
    pub path: PathBuf,
    /// The name shown: the variable, or the path with `~`.
    pub shown: String,
}

impl KeyPlace for KeyFile {
    fn name(&self) -> String {
        self.shown.clone()
    }
    fn get(&self) -> Result<Option<[u8; 32]>, String> {
        match self.fs.read(&self.path) {
            None => Ok(None),
            Some(b) => unhex(&String::from_utf8_lossy(&b)).map(Some).ok_or_else(|| format!("{} does not hold 64 hex digits", self.path.display())),
        }
    }
    fn put(&self, key: &[u8; 32]) -> Result<(), String> {
        self.fs.write_private(&self.path, hex(key).as_bytes())
    }
}

/// The master key in its place, for `wardian key` (ADR-2610081700).
pub struct KeyBackup {
    pub fs: Arc<dyn FileSystem>,
    pub data_dir: PathBuf,
    /// Where the key is kept; None when there is no place for it.
    pub place: Option<Box<dyn KeyPlace>>,
}

impl KeyBackup {
    fn place(&self) -> Result<&dyn KeyPlace, String> {
        self.place.as_deref().ok_or_else(|| "there is no place for a master key: set WARDIAN_MASTER_KEY_FILE".into())
    }

    /// (secrets `key` opens, secrets sealed in the data folder).
    fn opens(&self, key: &[u8; 32]) -> (usize, usize) {
        let cipher = cipher(key);
        let sealed: Vec<PathBuf> = self
            .fs
            .list_dir(&self.data_dir)
            .into_iter()
            .map(|n| self.data_dir.join(n))
            .filter(|p| self.fs.is_file(p))
            .filter(|p| self.fs.read(p).is_some_and(|b| b.starts_with(HEADER)))
            .collect();
        let opened = sealed.iter().filter(|p| self.fs.read(p).is_some_and(|b| SealedSecrets::unseal(&cipher, p, &b).is_ok())).count();
        (opened, sealed.len())
    }
}

impl MasterKey for KeyBackup {
    fn status(&self) -> Result<String, String> {
        let place = self.place()?;
        match place.get()? {
            Some(key) => {
                let (opened, sealed) = self.opens(&key);
                Ok(format!("The master key is in {}. It opens {opened} of the {sealed} secret(s) sealed in {}.", place.name(), self.data_dir.display()))
            }
            None => Ok(format!("There is no master key yet. Wardian makes one in {} at its next start.", place.name())),
        }
    }

    fn export(&self) -> Result<String, String> {
        let place = self.place()?;
        place.get()?.map(|k| hex(&k)).ok_or_else(|| format!("there is no master key in {} yet", place.name()))
    }

    fn import(&self, text: &str, force: bool) -> Result<String, String> {
        let key = unhex(text).ok_or("a master key is 64 hex digits")?;
        let place = self.place()?;
        let (opened, sealed) = self.opens(&key);
        if !force {
            if sealed > 0 && opened == 0 {
                return Err(format!("this key opens none of the {sealed} secret(s) sealed in {}. Add --force to keep it anyway.", self.data_dir.display()));
            }
            if let Some(current) = place.get()?.filter(|c| *c != key) {
                let (kept_opens, _) = self.opens(&current);
                return Err(format!("{} holds another master key, which opens {kept_opens} of the {sealed} sealed secret(s). Add --force to replace it.", place.name()));
            }
        }
        place.put(&key)?;
        Ok(format!("The master key is now in {}. It opens {opened} of the {sealed} secret(s) sealed in {}. Restart Wardian to use it.", place.name(), self.data_dir.display()))
    }
}

/// How secrets are kept this time Wardian runs.
enum Mode {
    Sealed { key: LessSafeKey, place: String },
    /// Nowhere to keep a master key, and nothing sealed yet: plain private files, as before.
    Plain { why: String },
    /// Sealed secrets exist, and their key cannot be found: they cannot be read or replaced.
    Broken { why: String },
}

pub struct SealedSecrets {
    fs: Arc<dyn FileSystem>,
    mode: Mode,
}

fn cipher(key: &[u8; 32]) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).expect("a 32-byte key"))
}

/// The file's name: the additional data that ties a sealed secret to its file.
fn aad_of(path: &Path) -> Vec<u8> {
    path.file_name().map(|n| n.to_string_lossy().into_owned().into_bytes()).unwrap_or_default()
}

impl SealedSecrets {
    /// Finds the master key in `places`, in order. When none has one and nothing in `data_dir` is
    /// sealed yet, makes one and keeps it in the first of `keep_in` that takes it. Never makes a
    /// new key while a sealed secret exists: its key is somewhere Wardian cannot see now.
    pub fn open(fs: Arc<dyn FileSystem>, data_dir: &Path, places: &[&dyn KeyPlace], keep_in: &[&dyn KeyPlace]) -> SealedSecrets {
        let mut trouble = Vec::new();
        for place in places {
            match place.get() {
                Ok(Some(key)) => return SealedSecrets { fs, mode: Mode::Sealed { key: cipher(&key), place: place.name() } },
                Ok(None) => {}
                Err(e) => trouble.push(format!("{}: {e}", place.name())),
            }
        }
        let sealed: Vec<String> = fs
            .list_dir(data_dir)
            .into_iter()
            .filter(|n| fs.is_file(&data_dir.join(n)) && fs.read(&data_dir.join(n)).is_some_and(|b| b.starts_with(HEADER)))
            .collect();
        if !sealed.is_empty() {
            let looked: Vec<String> = places.iter().map(|p| p.name()).collect();
            let mut why = format!("{} is sealed, but no master key was found in {}", sealed.join(", "), looked.join(", "));
            if !trouble.is_empty() {
                why += &format!(" ({})", trouble.join("; "));
            }
            return SealedSecrets { fs, mode: Mode::Broken { why } };
        }
        let key = random::<32>();
        for place in keep_in {
            match place.put(&key) {
                Ok(()) => return SealedSecrets { fs, mode: Mode::Sealed { key: cipher(&key), place: place.name() } },
                Err(e) => trouble.push(format!("{}: {e}", place.name())),
            }
        }
        let why = if trouble.is_empty() { "no place to keep a master key".to_string() } else { trouble.join("; ") };
        SealedSecrets { fs, mode: Mode::Plain { why } }
    }

    /// Secrets sealed with `key`, for tests.
    #[cfg(test)]
    pub fn with_key(fs: Arc<dyn FileSystem>, key: [u8; 32]) -> SealedSecrets {
        SealedSecrets { fs, mode: Mode::Sealed { key: cipher(&key), place: "a test key".into() } }
    }

    fn seal(key: &LessSafeKey, path: &Path, plain: &[u8]) -> Vec<u8> {
        let nonce = random::<NONCE_LEN>();
        let mut body = plain.to_vec();
        key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(aad_of(path)), &mut body).expect("sealing a secret");
        [HEADER, &nonce, &body].concat()
    }

    fn unseal(key: &LessSafeKey, path: &Path, sealed: &[u8]) -> Result<Vec<u8>, String> {
        let rest = &sealed[HEADER.len()..];
        if rest.len() < NONCE_LEN {
            return Err("the sealed file is cut short".into());
        }
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| "a bad nonce")?;
        let mut body = body.to_vec();
        let plain = key
            .open_in_place(nonce, Aad::from(aad_of(path)), &mut body)
            .map_err(|_| "it was sealed with another master key, or the file was changed".to_string())?;
        Ok(plain.to_vec())
    }
}

impl Secrets for SealedSecrets {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        let Some(bytes) = self.fs.read(path) else {
            return if self.fs.exists(path) { Err("the file is there, but it cannot be opened".into()) } else { Ok(None) };
        };
        match (&self.mode, bytes.starts_with(HEADER)) {
            (Mode::Sealed { key, .. }, true) => SealedSecrets::unseal(key, path, &bytes).map(Some),
            (Mode::Sealed { key, .. }, false) => {
                // Written before sealing: seal it now, and read it as it is.
                if let Err(e) = self.fs.write_private(path, &SealedSecrets::seal(key, path, &bytes)) {
                    eprintln!("secrets: could not seal {}: {e}", path.display());
                }
                Ok(Some(bytes))
            }
            (Mode::Plain { .. }, false) => Ok(Some(bytes)),
            (Mode::Plain { why } | Mode::Broken { why }, true) => Err(format!("it is sealed, and {why}")),
            (Mode::Broken { .. }, false) => Ok(Some(bytes)),
        }
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        match &self.mode {
            Mode::Sealed { key, .. } => self.fs.write_private(path, &SealedSecrets::seal(key, path, bytes)),
            Mode::Plain { .. } => self.fs.write_private(path, bytes),
            Mode::Broken { why } => Err(format!("secrets cannot be saved: {why}")),
        }
    }

    fn remove(&self, path: &Path) {
        self.fs.remove_file(path)
    }

    fn new_token(&self) -> String {
        new_token()
    }

    fn describe(&self) -> Value {
        match &self.mode {
            Mode::Sealed { place, .. } => json!({ "sealed": true, "master": place, "note": format!("sealed with AES-256-GCM, under a master key kept in {place}") }),
            Mode::Plain { why } => json!({ "sealed": false, "error": why,
                "note": format!("in files only their owner can read, NOT sealed: {why}. Set WARDIAN_MASTER_KEY_FILE to a file outside the data folder") }),
            Mode::Broken { why } => json!({ "sealed": true, "error": why, "note": format!("sealed, but they cannot be opened: {why}") }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::secondary::local_disk::LocalDisk;
    use std::sync::Mutex;

    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wardian-sealed-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A place in memory; `broken` answers every question with an error.
    #[derive(Default)]
    struct Memory {
        key: Mutex<Option<[u8; 32]>>,
        broken: bool,
    }

    impl KeyPlace for Memory {
        fn name(&self) -> String {
            "memory".into()
        }
        fn get(&self) -> Result<Option<[u8; 32]>, String> {
            if self.broken {
                return Err("locked".into());
            }
            Ok(*self.key.lock().unwrap())
        }
        fn put(&self, key: &[u8; 32]) -> Result<(), String> {
            if self.broken {
                return Err("locked".into());
            }
            *self.key.lock().unwrap() = Some(*key);
            Ok(())
        }
    }

    #[test]
    fn sealed_secrets_round_trip_and_hold_no_plain_text() {
        let d = dir("round");
        let s = SealedSecrets::with_key(Arc::new(LocalDisk), [7; 32]);
        s.write(&d.join("anthropic-key"), b"sk-ant-SECRET-VALUE").unwrap();
        let on_disk = std::fs::read(d.join("anthropic-key")).unwrap();
        assert!(on_disk.starts_with(HEADER));
        assert!(!on_disk.windows(6).any(|w| w == b"SECRET"), "the file holds no plain text");
        assert_eq!(s.read(&d.join("anthropic-key")).unwrap().unwrap(), b"sk-ant-SECRET-VALUE");
        s.write(&d.join("again"), b"x").unwrap();
        s.write(&d.join("again2"), b"x").unwrap();
        assert_ne!(std::fs::read(d.join("again")).unwrap()[HEADER.len()..], std::fs::read(d.join("again2")).unwrap()[HEADER.len()..], "every seal has its own nonce");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(d.join("anthropic-key")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sealed_secrets_do_not_open_with_another_key_another_name_or_a_changed_byte() {
        let d = dir("wrong");
        let s = SealedSecrets::with_key(Arc::new(LocalDisk), [7; 32]);
        s.write(&d.join("bedrock.json"), b"{\"token\":\"t\"}").unwrap();
        let other = SealedSecrets::with_key(Arc::new(LocalDisk), [8; 32]);
        assert!(other.read(&d.join("bedrock.json")).unwrap_err().contains("another master key"));
        std::fs::copy(d.join("bedrock.json"), d.join("anthropic-key")).unwrap();
        assert!(s.read(&d.join("anthropic-key")).is_err(), "a sealed file under another name does not open");
        let mut bytes = std::fs::read(d.join("bedrock.json")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(d.join("bedrock.json"), &bytes).unwrap();
        assert!(s.read(&d.join("bedrock.json")).is_err(), "a changed byte does not open");
        std::fs::write(d.join("short"), HEADER).unwrap();
        assert!(s.read(&d.join("short")).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sealed_secrets_seal_a_plain_secret_the_first_time_it_is_read() {
        let d = dir("migrate");
        std::fs::write(d.join("splunk.json"), b"{\"token\":\"OLD-PLAIN\"}").unwrap();
        let s = SealedSecrets::with_key(Arc::new(LocalDisk), [7; 32]);
        assert_eq!(s.read(&d.join("splunk.json")).unwrap().unwrap(), b"{\"token\":\"OLD-PLAIN\"}");
        assert!(std::fs::read(d.join("splunk.json")).unwrap().starts_with(HEADER), "sealed in place");
        assert_eq!(s.read(&d.join("splunk.json")).unwrap().unwrap(), b"{\"token\":\"OLD-PLAIN\"}");
        assert_eq!(s.read(&d.join("nothing")).unwrap(), None);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sealed_master_key_is_found_in_order_and_made_only_when_nothing_is_sealed() {
        let d = dir("master");
        let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
        let (first, second) = (Memory::default(), Memory::default());
        // Nothing anywhere: a key is made, kept in the first place that takes it.
        let s = SealedSecrets::open(Arc::clone(&fs), &d, &[&first, &second], &[&first, &second]);
        assert_eq!(s.describe()["sealed"], true);
        assert!(first.key.lock().unwrap().is_some() && second.key.lock().unwrap().is_none());
        s.write(&d.join("admin-token"), b"tok").unwrap();
        // Found again: the same key opens what was sealed.
        let again = SealedSecrets::open(Arc::clone(&fs), &d, &[&first, &second], &[&first]);
        assert_eq!(again.read(&d.join("admin-token")).unwrap().unwrap(), b"tok");
        // The key is gone, and a secret is sealed: no new key, and the reason is given.
        let empty = Memory::default();
        let lost = SealedSecrets::open(Arc::clone(&fs), &d, &[&empty], &[&empty]);
        assert!(empty.key.lock().unwrap().is_none(), "no new key while a sealed secret exists");
        let e = lost.read(&d.join("admin-token")).unwrap_err();
        assert!(e.contains("admin-token is sealed") && e.contains("no master key was found in memory"), "{e}");
        assert!(lost.write(&d.join("anthropic-key"), b"k").is_err(), "and nothing is saved that could not be read later");
        // A place that answers with an error is not a reason to make a new key either.
        let locked = Memory { broken: true, ..Default::default() };
        let e = SealedSecrets::open(Arc::clone(&fs), &d, &[&locked], &[&empty]).read(&d.join("admin-token")).unwrap_err();
        assert!(e.contains("memory: locked"), "{e}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sealed_secrets_stay_plain_when_no_place_can_keep_a_key() {
        let d = dir("plain");
        std::fs::write(d.join("splunk.json"), b"{}").unwrap();
        let locked = Memory { broken: true, ..Default::default() };
        let s = SealedSecrets::open(Arc::new(LocalDisk), &d, &[], &[&locked]);
        assert_eq!(s.describe()["sealed"], false);
        assert!(s.describe()["note"].as_str().unwrap().contains("WARDIAN_MASTER_KEY_FILE"));
        assert_eq!(s.read(&d.join("splunk.json")).unwrap().unwrap(), b"{}");
        s.write(&d.join("anthropic-key"), b"k").unwrap();
        assert_eq!(std::fs::read(d.join("anthropic-key")).unwrap(), b"k", "kept plain, as before");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sealed_key_places_read_hex_and_refuse_anything_else() {
        assert!(EnvKey("ab".repeat(32)).get().unwrap().is_some());
        assert!(EnvKey("zz".repeat(32)).get().is_err());
        assert!(EnvKey("ab".into()).get().is_err());
        let d = dir("file");
        let f = KeyFile { fs: Arc::new(LocalDisk), path: d.join("keys/master.key"), shown: "the key file".into() };
        assert_eq!(f.get().unwrap(), None);
        f.put(&[9; 32]).unwrap();
        assert_eq!(f.get().unwrap(), Some([9; 32]));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(d.join("keys/master.key")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::write(d.join("keys/master.key"), "not hex").unwrap();
        assert!(f.get().is_err());
        let _ = std::fs::remove_dir_all(d);
        let t = new_token();
        assert_eq!((t.len(), t.chars().all(|c| c.is_ascii_hexdigit())), (64, true));
        assert_ne!(t, new_token());
    }

    #[test]
    fn sealed_key_backup_exports_and_imports_only_a_key_that_opens_the_secrets() {
        let d = dir("backup");
        let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
        let file = |name: &str| KeyFile { fs: Arc::clone(&fs), path: d.join(name), shown: name.into() };
        let backup = |name: &str| KeyBackup { fs: Arc::clone(&fs), data_dir: d.join("data"), place: Some(Box::new(file(name))) };
        std::fs::create_dir_all(d.join("data")).unwrap();
        // No key yet.
        assert!(backup("master.key").status().unwrap().contains("no master key yet"));
        assert!(backup("master.key").export().is_err());
        // A key, and two secrets sealed with it.
        file("master.key").put(&[4; 32]).unwrap();
        let s = SealedSecrets::with_key(Arc::clone(&fs), [4; 32]);
        s.write(&d.join("data/anthropic-key"), b"k").unwrap();
        s.write(&d.join("data/admin-token"), b"t").unwrap();
        assert!(backup("master.key").status().unwrap().contains("opens 2 of the 2"));
        let exported = backup("master.key").export().unwrap();
        assert_eq!(exported, "04".repeat(32));
        // Restored into a new place: opens both.
        let out = backup("restored.key").import(&exported, false).unwrap();
        assert!(out.contains("opens 2 of the 2"), "{out}");
        assert_eq!(file("restored.key").get().unwrap(), Some([4; 32]));
        // A key that opens nothing, or that replaces another, needs --force.
        let wrong = "05".repeat(32);
        assert!(backup("other.key").import(&wrong, false).unwrap_err().contains("opens none of the 2"));
        assert!(backup("other.key").import(&wrong, true).is_ok());
        assert!(backup("restored.key").import(&wrong, false).is_err(), "a different key is not replaced without --force");
        assert!(backup("x.key").import("not hex", true).is_err());
        let env = KeyBackup { fs: Arc::clone(&fs), data_dir: d.join("data"), place: Some(Box::new(EnvKey("04".repeat(32)))) };
        assert_eq!(env.export().unwrap(), "04".repeat(32));
        assert!(env.import(&exported, true).is_err(), "WARDIAN_MASTER_KEY is never written");
        let _ = std::fs::remove_dir_all(d);
    }
}
