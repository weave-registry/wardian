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

/// Gives the working folder every example app it has never had (ADR-2610081600): one that is not
/// there, not in the trash and not listed in `.examples-seen`, so an app the user removed stays
/// removed and an example new in this version reaches an old folder. The examples come from
/// `source` on disk when there is one (a checkout's `./apps`, or the copy installed beside the
/// program), otherwise from the copies built into the program, `built_in`. Neither is changed.
/// Returns the names added.
pub fn add_examples(fs: &dyn FileSystem, source: Option<&Path>, built_in: &[(&str, &[u8])], working: &Path) -> Result<Vec<String>, String> {
    let source = source.filter(|s| fs.is_dir(s));
    let examples = example_names(fs, source, built_in);
    fs.create_dir_all(working)?;
    let seen = fs.read(&working.join(EXAMPLES_SEEN)).map(|b| String::from_utf8_lossy(&b).lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let trashed = fs.list_dir(&working.join(".trash"));
    let mut added = Vec::new();
    for name in &examples {
        let removed = trashed.iter().any(|t| t.strip_prefix(name.as_str()).is_some_and(|rest| rest.starts_with("--")));
        if fs.exists(&working.join(name)) || removed || seen.contains(name) {
            continue;
        }
        match source {
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
    let mut list: Vec<String> = seen.into_iter().chain(examples).collect();
    list.sort();
    list.dedup();
    fs.write(&working.join(EXAMPLES_SEEN), format!("{}\n", list.join("\n")).as_bytes())?;
    Ok(added)
}

/// The names of the example apps: the apps in `source` on disk when there is one, otherwise the
/// ones built into the program, `built_in` (ADR-2610081600).
pub fn example_names(fs: &dyn FileSystem, source: Option<&Path>, built_in: &[(&str, &[u8])]) -> Vec<String> {
    let is_app = |dir: &Path| APP_MARKERS.iter().any(|m| fs.is_file(&dir.join(m)));
    match source.filter(|s| fs.is_dir(s)) {
        Some(src) => fs.list_dir(src).into_iter().filter(|n| safe_segment(n) && is_app(&src.join(n))).collect(),
        None => {
            let mut names: Vec<String> = built_in.iter().filter_map(|(p, _)| p.split('/').next().map(String::from)).collect();
            names.dedup();
            names
        }
    }
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
