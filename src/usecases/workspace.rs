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

/// The file in the working folder listing every example app Wardian has offered, one name per line.
const EXAMPLES_SEEN: &str = ".examples-seen";

/// Fills the working folder from `source`, the example apps (the repository's `./apps` in a
/// checkout, or those installed beside the program). An empty or missing working folder gets all
/// of them. Otherwise each example app the folder has never had is added: one that is not there,
/// not in the trash and not listed in `.examples-seen`, so an app the user removed stays removed.
/// `source` is never changed. Returns how many apps were added, or None when there was nothing to add.
pub fn seed(fs: &dyn FileSystem, source: &Path, working: &Path) -> Result<Option<usize>, String> {
    if !fs.is_dir(source) {
        fs.create_dir_all(working)?;
        return Ok(None);
    }
    let is_app = |dir: &Path| APP_MARKERS.iter().any(|m| fs.is_file(&dir.join(m)));
    let examples: Vec<String> = fs.list_dir(source).into_iter().filter(|n| !n.starts_with('.') && is_app(&source.join(n))).collect();
    let added = if !fs.exists(working) || fs.list_dir(working).is_empty() {
        copy_tree(fs, source, working)?;
        fs.list_dir(working).into_iter().filter(|n| is_app(&working.join(n))).count()
    } else {
        let seen = fs.read(&working.join(EXAMPLES_SEEN)).map(|b| String::from_utf8_lossy(&b).lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
        let trashed = fs.list_dir(&working.join(".trash"));
        let mut n = 0;
        for name in &examples {
            let removed = trashed.iter().any(|t| t.strip_prefix(name.as_str()).is_some_and(|rest| rest.starts_with("--")));
            if fs.exists(&working.join(name)) || removed || seen.contains(name) {
                continue;
            }
            copy_tree(fs, &source.join(name), &working.join(name))?;
            n += 1;
        }
        n
    };
    let mut list = examples.join("\n");
    list.push('\n');
    fs.write(&working.join(EXAMPLES_SEEN), list.as_bytes())?;
    Ok((added > 0).then_some(added))
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
