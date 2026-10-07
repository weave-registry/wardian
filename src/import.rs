//! Copies apps out of a zip file into the local apps folder.
//!
//! An app is a folder with a WebAssembly module in it, plus whatever else
//! its page needs. Its folder tree is kept as it is, so relative links like
//! `../pkg/usl.js` keep working. The app's top folder is found like this:
//!
//!   - the nearest folder above the module that holds an app.json, else
//!   - the module's folder, skipping build folders (pkg/, dist/, ...), so
//!     usl-wasm/pkg/usl_wasm.wasm gives the app "usl-wasm".
//!
//! If that folder has no app.wasm, a copy of the module is added under that
//! name, which is how the server recognises an app. A folder with several
//! .wasm files and no app.wasm is ambiguous and skipped.
//!
//! A folder with a suite.json is a suite: its whole tree is one app, and
//! any modules inside it belong to it.

use crate::source::{safe_segment, SKIP_DIRS};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 10_000;
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// Folder names that say where a build went, not what the app is.
const GENERIC_DIRS: &[&str] = &["pkg", "dist", "build", "out", "output", "release", "wasm", "bin", "www", "public"];

#[derive(Serialize)]
pub struct Imported {
    pub apps: Vec<String>,
    /// Entries left out, with the reason, so the user can see what happened.
    pub skipped: Vec<String>,
}

/// Turns a file name like "My Apps (v2).zip" into a usable app name.
pub fn app_name_from(file_name: &str) -> String {
    let stem = file_name.rsplit('/').next().unwrap_or("");
    let stem = [".wardian", ".rustle", ".zip", ".wasm"].iter().find_map(|x| stem.strip_suffix(x)).unwrap_or(stem);
    let clean: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' })
        .collect();
    clean.trim_matches(|c| c == '-' || c == '.').to_string()
}

struct Plan {
    name: String,
    /// (zip entry index, path inside the app to write it to)
    files: Vec<(usize, String)>,
}

/// A zip entry that survived the path checks, split into its folder parts.
struct Entry {
    index: usize,
    parts: Vec<String>,
}

pub fn import_zip(bytes: &[u8], zip_name: &str, root: &Path, replace: bool) -> Result<Imported, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a valid zip: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("zip has more than {MAX_ENTRIES} entries"));
    }
    let mut skipped = Vec::new();
    let entries = read_entries(&mut zip, &mut skipped)?;
    let mut plans = plan(&entries, zip_name, &mut skipped)?;
    plans.sort_by(|a, b| a.name.cmp(&b.name));

    let exists: Vec<&str> = plans
        .iter()
        .map(|p| p.name.as_str())
        .filter(|n| root.join(n).exists())
        .collect();
    if !exists.is_empty() && !replace {
        return Err(format!("already in the local folder: {} (tick Replace to overwrite)", exists.join(", ")));
    }

    // Unpack everything into a hidden staging folder first, so a bad file
    // part-way through leaves the apps folder exactly as it was.
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let staging = root.join(format!(".import-{nanos}"));
    let result = stage(&mut zip, &plans, &staging).and_then(|()| commit(&plans, &staging, root));
    let _ = fs::remove_dir_all(&staging);
    result?;

    Ok(Imported { apps: plans.into_iter().map(|p| p.name).collect(), skipped })
}

/// Lists the files in the zip as checked name parts. Entry paths are never
/// joined onto a disk path as they are, so a hostile "../../x" entry cannot
/// write outside the apps folder.
fn read_entries(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, skipped: &mut Vec<String>) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for index in 0..zip.len() {
        let f = zip.by_index(index).map_err(|e| format!("reading zip: {e}"))?;
        if f.is_dir() {
            continue;
        }
        let parts: Option<Vec<String>> = f.enclosed_name().and_then(|path| {
            path.components()
                .map(|c| match c {
                    Component::Normal(s) => s.to_str().map(str::to_string),
                    _ => None,
                })
                .collect()
        });
        let Some(parts) = parts.filter(|p| !p.is_empty()) else {
            skipped.push(format!("{} (unsafe path)", f.name()));
            continue;
        };
        // Hidden files, macOS's __MACOSX/ copies and build caches are not app files.
        if parts.iter().any(|p| p.starts_with('.') || p == "__MACOSX" || SKIP_DIRS.contains(&p.as_str())) {
            continue;
        }
        out.push(Entry { index, parts });
    }
    Ok(out)
}

fn plan(entries: &[Entry], zip_name: &str, skipped: &mut Vec<String>) -> Result<Vec<Plan>, String> {
    // Group file names by folder to find the modules.
    let mut dirs: BTreeMap<Vec<String>, Vec<&Entry>> = BTreeMap::new();
    for e in entries {
        dirs.entry(e.parts[..e.parts.len() - 1].to_vec()).or_default().push(e);
    }
    let has_manifest: HashSet<&[String]> = dirs
        .iter()
        .filter(|(_, files)| files.iter().any(|e| matches!(e.parts.last().unwrap().as_str(), "app.json" | "suite.json")))
        .map(|(d, _)| d.as_slice())
        .collect();

    // app top folder -> (app name, the module entry if it is a wasm app)
    let mut roots: BTreeMap<Vec<String>, (String, Option<&Entry>)> = BTreeMap::new();
    // Suites first, so a module inside a suite joins the suite.
    for (dir, files) in &dirs {
        if files.iter().any(|e| e.parts.last().unwrap() == "suite.json") {
            let name = app_name_from(dir.last().map(String::as_str).unwrap_or(zip_name));
            if safe_segment(&name) {
                roots.insert(dir.clone(), (name, None));
            } else {
                skipped.push(format!("{}/suite.json (cannot make an app name from it)", dir.join("/")));
            }
        }
    }
    for (dir, files) in &dirs {
        let wasm: Vec<&&Entry> = files.iter().filter(|e| e.parts.last().unwrap().ends_with(".wasm")).collect();
        let main = match wasm.iter().find(|e| e.parts.last().unwrap() == "app.wasm") {
            Some(e) => **e,
            None if wasm.len() == 1 => *wasm[0],
            None => {
                if wasm.len() > 1 {
                    let at = if dir.is_empty() { "the top of the zip".to_string() } else { dir.join("/") };
                    skipped.push(format!("{at} (several .wasm files and no app.wasm)"));
                }
                continue;
            }
        };
        let top = (0..=dir.len())
            .rev()
            .find(|&n| has_manifest.contains(&dir[..n]))
            .unwrap_or_else(|| {
                let generic = dir.iter().rev().take_while(|d| GENERIC_DIRS.contains(&d.to_ascii_lowercase().as_str())).count();
                dir.len() - generic
            });
        let top = dir[..top].to_vec();
        let file = main.parts.last().unwrap();
        let name = match top.last() {
            Some(last) => app_name_from(last),
            // A module at the very top: name it after the module, unless it
            // is called app.wasm or sits in a build folder.
            None if dir.is_empty() && file != "app.wasm" => app_name_from(file),
            None => app_name_from(zip_name),
        };
        if !safe_segment(&name) {
            skipped.push(format!("{} (cannot make an app name from it)", main.parts.join("/")));
            continue;
        }
        // Several modules can share a top folder (say pkg/ and dist/); the
        // first one found becomes the app.wasm.
        roots.entry(top).or_insert((name, Some(main)));
    }
    if roots.is_empty() {
        return Err("no app found in the zip (expected a .wasm file or a suite.json)".into());
    }
    let mut names = HashSet::new();
    for (name, _) in roots.values() {
        if !names.insert(name) {
            return Err(format!("the zip holds two apps named \"{name}\""));
        }
    }

    // Each file belongs to the deepest app folder it sits in.
    let mut plans: BTreeMap<&Vec<String>, Plan> = BTreeMap::new();
    let mut outside = 0;
    for e in entries {
        let Some(top) = roots.keys().filter(|t| e.parts.starts_with(t) && e.parts.len() > t.len()).max_by_key(|t| t.len()) else {
            outside += 1;
            continue;
        };
        let rel = &e.parts[top.len()..];
        if !rel.iter().all(|p| safe_segment(p)) {
            skipped.push(format!("{} (file name not allowed)", e.parts.join("/")));
            continue;
        }
        let name = roots[top].0.clone();
        plans.entry(top).or_insert_with(|| Plan { name, files: Vec::new() }).files.push((e.index, rel.join("/")));
    }
    if outside > 0 {
        skipped.push(format!("{outside} file(s) outside any app folder"));
    }
    for (top, (_, main)) in &roots {
        let Some(main) = main else { continue };
        let plan = plans.get_mut(top).expect("an app folder holds at least its module");
        if !plan.files.iter().any(|(_, rel)| rel == "app.wasm") {
            plan.files.push((main.index, "app.wasm".into()));
        }
    }
    Ok(plans.into_values().collect())
}

fn stage(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, plans: &[Plan], staging: &Path) -> Result<(), String> {
    let mut total = 0u64;
    for plan in plans {
        let dir = staging.join(&plan.name);
        for (i, rel) in &plan.files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
            }
            let entry = zip.by_index(*i).map_err(|e| format!("reading zip: {e}"))?;
            // Count the bytes actually unpacked; the sizes in a zip header
            // are only claims, and a zip bomb lies about them.
            let mut buf = Vec::new();
            entry
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut buf)
                .map_err(|e| format!("unpacking {}/{rel}: {e}", plan.name))?;
            if buf.len() as u64 > MAX_FILE_BYTES {
                return Err(format!("{}/{rel} is larger than 64 MB", plan.name));
            }
            total += buf.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err("the zip unpacks to more than 256 MB".into());
            }
            if rel.ends_with(".wasm") && !buf.starts_with(b"\0asm") {
                return Err(format!("{}/{rel} is not WebAssembly", plan.name));
            }
            fs::write(&path, &buf).map_err(|e| format!("writing {}/{rel}: {e}", plan.name))?;
        }
    }
    Ok(())
}

fn commit(plans: &[Plan], staging: &Path, root: &Path) -> Result<(), String> {
    for plan in plans {
        let target = root.join(&plan.name);
        if target.exists() {
            // Move the old copy aside before deleting it, so the app is
            // never missing for longer than one rename.
            let old = staging.join(format!(".old-{}", plan.name));
            fs::rename(&target, &old).map_err(|e| format!("replacing {}: {e}", plan.name))?;
        }
        fs::rename(staging.join(&plan.name), &target).map_err(|e| format!("installing {}: {e}", plan.name))?;
    }
    Ok(())
}
