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

/// On the first start, fills an empty or missing working folder from `source` (the repository's
/// `./apps` when Wardian runs from a checkout). `source` is never changed. Returns how many apps
/// were copied, or None when there was nothing to do.
pub fn seed(fs: &dyn FileSystem, source: &Path, working: &Path) -> Result<Option<usize>, String> {
    if fs.exists(working) && !fs.list_dir(working).is_empty() {
        return Ok(None);
    }
    if !fs.is_dir(source) {
        fs.create_dir_all(working)?;
        return Ok(None);
    }
    copy_tree(fs, source, working)?;
    let apps = fs.list_dir(working).into_iter().filter(|n| APP_MARKERS.iter().any(|m| fs.is_file(&working.join(n).join(m)))).count();
    Ok(Some(apps))
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
