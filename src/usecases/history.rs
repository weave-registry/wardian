//! The history of each app (ADR-2610071122), in `<data dir>/history/<app>/`: one numbered snapshot
//! per version and `log.json` saying when, by what and why. Every save records a version; any
//! version can be compared with the current app and restored, as a new version.

use super::workspace::copy_tree;
use crate::domain::history::{change, kept_in_copies, line_diff, next_number, prune, Change, Version, KEEP_VERSIONS};
use crate::domain::package::{safe_segment, APP_MARKERS};
use crate::ports::{clock::Clock, service::AppHistory, storage::FileSystem};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A text file larger than this is listed but not diffed.
const MAX_DIFF_BYTES: usize = 512 * 1024;

pub struct History {
    fs: Arc<dyn FileSystem>,
    dir: PathBuf,
    apps: PathBuf,
    /// One change to the history at a time.
    lock: Mutex<()>,
    clock: Arc<dyn Clock>,
}

impl History {
    pub fn new(fs: Arc<dyn FileSystem>, data_dir: &Path, apps: &Path, clock: Arc<dyn Clock>) -> History {
        History { fs, dir: data_dir.join("history"), apps: apps.to_path_buf(), lock: Mutex::new(()), clock }
    }

    fn app_dir(&self, app: &str) -> PathBuf {
        self.dir.join(app)
    }

    fn log(&self, app: &str) -> Vec<Version> {
        self.fs.read(&self.app_dir(app).join("log.json")).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_log(&self, app: &str, log: &[Version]) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(log).map_err(|e| e.to_string())?;
        self.fs.write_private(&self.app_dir(app).join("log.json"), &bytes).map_err(|e| format!("saving the history of {app}: {e}"))
    }

    fn is_app(&self, dir: &Path) -> bool {
        APP_MARKERS.iter().any(|m| self.fs.is_file(&dir.join(m)))
    }

    /// Adds a version made from the app folder as it is now. The lock must be held.
    fn add(&self, app: &str, by: &str, why: &str) -> Result<u64, String> {
        let mut log = self.log(app);
        let n = next_number(&log);
        copy_tree(&*self.fs, &self.apps.join(app), &self.app_dir(app).join(n.to_string()))?;
        log.push(Version { n, at: self.clock.now(), by: by.into(), why: why.chars().take(500).collect() });
        for old in prune(&mut log, KEEP_VERSIONS) {
            self.fs.remove_dir_all(&self.app_dir(app).join(old.to_string()));
            // The data an import replaced is kept next to its version, and goes with it.
            self.fs.remove_dir_all(&self.app_dir(app).join(format!("{old}.data")));
        }
        self.write_log(app, &log)?;
        Ok(n)
    }

    /// Before the first change to an app with no history, keeps the folder as it is as version 1,
    /// so the original can always come back. Returns the latest version number, if any.
    pub fn before_change(&self, app: &str) -> Option<u64> {
        if !safe_segment(app) {
            return None;
        }
        let _guard = self.lock.lock().unwrap();
        let log = self.log(app);
        if let Some(last) = log.iter().map(|v| v.n).max() {
            return Some(last);
        }
        if !self.is_app(&self.apps.join(app)) {
            return None;
        }
        match self.add(app, "first-seen", "the app as it was before Wardian first changed it") {
            Ok(n) => Some(n),
            Err(e) => {
                eprintln!("history: {app}: {e}");
                None
            }
        }
    }

    /// Whether the app has any version in its history: it was changed, or restored, through Wardian.
    pub fn has_versions(&self, app: &str) -> bool {
        safe_segment(app) && !self.log(app).is_empty()
    }

    /// Records the app folder as it is now, after a save. A failure is reported, not fatal: the
    /// save itself already happened.
    pub fn record(&self, app: &str, by: &str, why: &str) -> Option<u64> {
        if !safe_segment(app) || !self.is_app(&self.apps.join(app)) {
            return None;
        }
        let _guard = self.lock.lock().unwrap();
        match self.add(app, by, why) {
            Ok(n) => {
                println!("history: {app} version {n} ({by})");
                Some(n)
            }
            Err(e) => {
                eprintln!("history: {app}: {e}");
                None
            }
        }
    }

    fn files(&self, dir: &Path) -> BTreeSet<String> {
        self.fs.walk(dir).into_iter().map(|(r, _)| r).filter(|r| kept_in_copies(r)).collect()
    }

    fn version_dir(&self, app: &str, n: u64) -> Result<PathBuf, String> {
        if !safe_segment(app) {
            return Err(format!("\"{app}\" is not an app name"));
        }
        if !self.log(app).iter().any(|v| v.n == n) {
            return Err(format!("{app} has no version {n}"));
        }
        Ok(self.app_dir(app).join(n.to_string()))
    }
}

impl AppHistory for History {
    fn versions(&self, app: &str) -> Result<Value, String> {
        if !safe_segment(app) {
            return Err(format!("\"{app}\" is not an app name"));
        }
        let mut log = self.log(app);
        log.sort_by(|a, b| b.n.cmp(&a.n));
        let latest = log.first().map(|v| v.n);
        Ok(json!({ "app": app, "versions": log.iter().map(|v| json!({
            "n": v.n, "at": v.at, "by": v.by, "why": v.why, "current": Some(v.n) == latest,
        })).collect::<Vec<_>>() }))
    }

    /// What differs between version `n` and the app as it is now: each file, and a line diff
    /// for text.
    fn diff(&self, app: &str, n: u64) -> Result<Value, String> {
        let old_dir = self.version_dir(app, n)?;
        let cur_dir = self.apps.join(app);
        let names: BTreeSet<String> = self.files(&old_dir).union(&self.files(&cur_dir)).cloned().collect();
        let mut files = Vec::new();
        for rel in names {
            let (old, cur) = (self.fs.read(&old_dir.join(&rel)), self.fs.read(&cur_dir.join(&rel)));
            let status = change(old.as_deref(), cur.as_deref());
            if status == Change::Same {
                continue;
            }
            let (a, b) = (old.unwrap_or_default(), cur.unwrap_or_default());
            let diff = if a.len().max(b.len()) > MAX_DIFF_BYTES {
                Value::String("(too large to compare line by line)".into())
            } else {
                line_diff(&a, &b).map(Value::String).unwrap_or_else(|| Value::String("(not text)".into()))
            };
            files.push(json!({ "path": rel, "status": status, "diff": diff }));
        }
        Ok(json!({ "app": app, "n": n, "files": files }))
    }

    /// Puts version `n` back, then records it as a new version, so the restore can be undone too.
    fn restore(&self, app: &str, n: u64) -> Result<Value, String> {
        let from = self.version_dir(app, n)?;
        let target = self.apps.join(app);
        self.before_change(app);
        let staging = self.apps.join(format!(".restore-{}-{:08x}", self.clock.now(), self.clock.nonce()));
        copy_tree(&*self.fs, &from, &staging)?;
        if !self.is_app(&staging) {
            self.fs.remove_dir_all(&staging);
            return Err(format!("version {n} of {app} is not a complete app"));
        }
        // Move the current copy aside first, so the app is never missing for longer than a rename.
        let old = self.apps.join(format!(".old-{}-{:08x}", app, self.clock.nonce()));
        if self.fs.exists(&target) {
            self.fs.rename(&target, &old).map_err(|e| format!("replacing {app}: {e}"))?;
        }
        if let Err(e) = self.fs.rename(&staging, &target) {
            let _ = self.fs.rename(&old, &target);
            self.fs.remove_dir_all(&staging);
            return Err(format!("restoring {app}: {e}"));
        }
        self.fs.remove_dir_all(&old);
        let new = self.record(app, "restore", &format!("restored version {n}"));
        println!("history: {app} restored to version {n}");
        Ok(json!({ "app": app, "restored": n, "version": new }))
    }
}
