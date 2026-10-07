//! Copies apps out of a zip file into a folder (the local apps folder, or a temporary one for
//! `wardian check`). Which files go where is the domain's plan; this reads the zip and writes.

use crate::domain::import_plan::{is_app_file, plan, Entry, Plan, MAX_ENTRIES, MAX_FILE_BYTES, MAX_TOTAL_BYTES};
use crate::domain::package::{random_u32, unix_now};
use crate::ports::storage::FileSystem;
use serde::Serialize;
use std::{
    io::{Cursor, Read},
    path::{Component, Path},
};

#[derive(Serialize)]
pub struct Imported {
    pub apps: Vec<String>,
    /// Entries left out, with the reason, so the user can see what happened.
    pub skipped: Vec<String>,
}

/// `before_replace` is called with the name of each app an import is about to replace, so its
/// history can keep the version that is going away.
pub fn import_zip(fs: &dyn FileSystem, bytes: &[u8], zip_name: &str, root: &Path, replace: bool, before_replace: &dyn Fn(&str)) -> Result<Imported, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a valid zip: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("zip has more than {MAX_ENTRIES} entries"));
    }
    let mut skipped = Vec::new();
    let entries = read_entries(&mut zip, &mut skipped)?;
    let plans = plan(&entries, zip_name, &mut skipped)?;

    let exists: Vec<&str> = plans.iter().map(|p| p.name.as_str()).filter(|n| fs.exists(&root.join(n))).collect();
    if !exists.is_empty() && !replace {
        return Err(format!("already in the local folder: {} (tick Replace to overwrite)", exists.join(", ")));
    }

    // Unpack everything into a hidden staging folder first, so a bad file
    // part-way through leaves the apps folder exactly as it was.
    let staging = root.join(format!(".import-{}-{:08x}", unix_now(), random_u32()));
    let result = stage(fs, &mut zip, &plans, &staging).and_then(|()| commit(fs, &plans, &staging, root, before_replace));
    fs.remove_dir_all(&staging);
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
        if is_app_file(&parts) {
            out.push(Entry { index, parts });
        }
    }
    Ok(out)
}

fn stage(fs: &dyn FileSystem, zip: &mut zip::ZipArchive<Cursor<&[u8]>>, plans: &[Plan], staging: &Path) -> Result<(), String> {
    let mut total = 0u64;
    for plan in plans {
        let dir = staging.join(&plan.name);
        for (i, rel) in &plan.files {
            let entry = zip.by_index(*i).map_err(|e| format!("reading zip: {e}"))?;
            // Count the bytes actually unpacked; the sizes in a zip header
            // are only claims, and a zip bomb lies about them.
            let mut buf = Vec::new();
            entry.take(MAX_FILE_BYTES + 1).read_to_end(&mut buf).map_err(|e| format!("unpacking {}/{rel}: {e}", plan.name))?;
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
            fs.write(&dir.join(rel), &buf).map_err(|e| format!("writing {}/{rel}: {e}", plan.name))?;
        }
    }
    Ok(())
}

fn commit(fs: &dyn FileSystem, plans: &[Plan], staging: &Path, root: &Path, before_replace: &dyn Fn(&str)) -> Result<(), String> {
    for plan in plans {
        let target = root.join(&plan.name);
        if fs.exists(&target) {
            before_replace(&plan.name);
            // Move the old copy aside before deleting it, so the app is
            // never missing for longer than one rename.
            let old = staging.join(format!(".old-{}", plan.name));
            fs.rename(&target, &old).map_err(|e| format!("replacing {}: {e}", plan.name))?;
        }
        fs.rename(&staging.join(&plan.name), &target).map_err(|e| format!("installing {}: {e}", plan.name))?;
    }
    Ok(())
}
