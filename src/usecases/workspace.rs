//! The working folder of apps (ADR-2610071122): where Wardian serves and saves apps, kept apart
//! from the repository's example apps. Copies between folders, the first-start seeding, the
//! "inside a git work tree" notice, and `wardian promote`.

use crate::domain::history::{change, kept_in_copies, Change, Promoted};
use crate::domain::package::{safe_segment, APP_MARKERS};
use crate::ports::storage::FileSystem;
use std::path::Path;

/// Copies every file under `from` into `to` (created), leaving out build output and repositories.
pub fn copy_tree(fs: &dyn FileSystem, from: &Path, to: &Path) -> Result<usize, String> {
    fs.create_dir_all(to)?;
    let mut n = 0;
    for (rel, _) in fs.walk(from) {
        if !kept_in_copies(&rel) {
            continue;
        }
        let bytes = fs.read(&from.join(&rel)).ok_or_else(|| format!("cannot read {}", from.join(&rel).display()))?;
        fs.write(&to.join(&rel), &bytes).map_err(|e| format!("writing {}: {e}", to.join(&rel).display()))?;
        n += 1;
    }
    Ok(n)
}

/// The file in the data folder naming every example app the working folder has been given.
const EXAMPLES_GIVEN: &str = "examples.json";

/// Gives the working folder each example app it has not been given before (ADR-2610081600): from
/// `source` on disk when there is one (a checkout's `./apps`, or the copy beside an installed
/// program), otherwise from the copies built into the program. `examples.json` remembers what was
/// given, so an example the user removed stays removed, and an example new in this version arrives
/// in a working folder that is years old. An app already in the folder is never replaced. Returns
/// the names added.
pub fn add_examples(fs: &dyn FileSystem, source: Option<&Path>, built_in: &[(&str, &[u8])], working: &Path, data_dir: &Path) -> Result<Vec<String>, String> {
    let record = data_dir.join(EXAMPLES_GIVEN);
    let mut given: Vec<String> = fs.read(&record).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let examples: Vec<String> = match source.filter(|s| fs.is_dir(s)) {
        Some(src) => fs.list_dir(src).into_iter().filter(|n| safe_segment(n) && APP_MARKERS.iter().any(|m| fs.is_file(&src.join(n).join(m)))).collect(),
        None => {
            let mut names: Vec<String> = built_in.iter().filter_map(|(p, _)| p.split('/').next().map(String::from)).collect();
            names.dedup();
            names
        }
    };
    fs.create_dir_all(working)?;
    let mut added = Vec::new();
    for name in &examples {
        if given.contains(name) || fs.exists(&working.join(name)) {
            continue;
        }
        match source.filter(|s| fs.is_dir(s)) {
            Some(src) => {
                copy_tree(fs, &src.join(name), &working.join(name))?;
            }
            None => {
                for (path, bytes) in built_in.iter().filter(|(p, _)| p.split('/').next() == Some(name.as_str())) {
                    fs.write(&working.join(path), bytes).map_err(|e| format!("writing {}: {e}", working.join(path).display()))?;
                }
            }
        }
        added.push(name.clone());
    }
    for name in examples {
        if !given.contains(&name) {
            given.push(name);
        }
    }
    given.sort();
    fs.write(&record, &serde_json::to_vec_pretty(&given).map_err(|e| e.to_string())?)?;
    Ok(added)
}

/// Proves Wardian can write in `data_dir` by writing and removing a small file, before anything
/// else uses the folder. A folder it cannot write would otherwise fail later, one save at a time.
pub fn check_writable(fs: &dyn FileSystem, data_dir: &Path) -> Result<(), String> {
    let probe = data_dir.join(".write-test");
    fs.create_dir_all(data_dir)?;
    fs.write(&probe, b"Wardian checks it can write here when it starts. Safe to delete.\n")?;
    fs.remove_file(&probe);
    Ok(())
}

/// The file in the data folder that says the first-run setup has not been seen (ADR-2610072033).
pub const FIRST_RUN_MARKER: &str = "first-run";

/// On a start with a missing or empty data folder, leaves the first-run marker there, before
/// anything else writes to it. Returns whether this is such a first start.
pub fn mark_first_run(fs: &dyn FileSystem, data_dir: &Path) -> Result<bool, String> {
    if fs.exists(data_dir) && !fs.list_dir(data_dir).is_empty() {
        return Ok(false);
    }
    fs.write(&data_dir.join(FIRST_RUN_MARKER), b"The first-run setup has not been shown yet. Wardian removes this file when it is skipped or finished.\n")?;
    Ok(true)
}

/// Whether `folder` is inside a git work tree: it or a parent holds `.git`.
pub fn inside_git(fs: &dyn FileSystem, folder: &Path) -> bool {
    let Some(abs) = fs.canonical(folder) else { return false };
    abs.ancestors().any(|p| fs.exists(&p.join(".git")))
}

/// Copies `app` from the working folder into the source folder, replacing the source's copy, so
/// a change made in the app can be reviewed as a diff and committed.
pub fn promote(fs: &dyn FileSystem, app: &str, working: &Path, source: &Path) -> Result<Promoted, String> {
    if !safe_segment(app) {
        return Err(format!("\"{app}\" is not an app name"));
    }
    let from = working.join(app);
    if !APP_MARKERS.iter().any(|m| fs.is_file(&from.join(m))) {
        return Err(format!("no app \"{app}\" in {}", working.display()));
    }
    let to = source.join(app);
    let files = |dir: &Path| -> Vec<String> { fs.walk(dir).into_iter().map(|(r, _)| r).filter(|r| kept_in_copies(r)).collect() };
    let (new, old) = (files(&from), files(&to));
    let mut out = Promoted::default();
    for rel in &new {
        match change(fs.read(&to.join(rel)).as_deref(), fs.read(&from.join(rel)).as_deref()) {
            Change::Added => out.added.push(rel.clone()),
            Change::Changed => out.changed.push(rel.clone()),
            _ => {}
        }
    }
    out.removed = old.into_iter().filter(|r| !new.contains(r)).collect();
    for rel in &out.removed {
        fs.remove_file(&to.join(rel));
    }
    for rel in out.added.iter().chain(&out.changed) {
        let bytes = fs.read(&from.join(rel)).ok_or_else(|| format!("cannot read {rel}"))?;
        fs.write(&to.join(rel), &bytes).map_err(|e| format!("writing {rel}: {e}"))?;
    }
    Ok(out)
}
