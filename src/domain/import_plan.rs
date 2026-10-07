//! Which apps a zip holds, and where each of its files goes.
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

use super::package::{safe_segment, SKIP_DIRS};
use std::collections::{BTreeMap, HashSet};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// The largest zip the host downloads or accepts.
pub const MAX_ZIP_BYTES: u64 = 100 * 1024 * 1024;

/// Folder names that say where a build went, not what the app is.
const GENERIC_DIRS: &[&str] = &["pkg", "dist", "build", "out", "output", "release", "wasm", "bin", "www", "public"];

/// Turns a file name like "My Apps (v2).zip" into a usable app name.
pub fn app_name_from(file_name: &str) -> String {
    let stem = file_name.rsplit('/').next().unwrap_or("");
    let stem = [".wardian", ".rustle", ".zip", ".wasm"].iter().find_map(|x| stem.strip_suffix(x)).unwrap_or(stem);
    let clean: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' }).collect();
    clean.trim_matches(|c| c == '-' || c == '.').to_string()
}

/// The folder parts of a zip entry's name. A name that is absolute, starts with a drive, climbs out
/// of its folder with `..` or holds a NUL is not a mistake but a hostile file, so the whole zip is
/// refused, not just the entry. `\` counts as a folder separator, as Windows tools write it.
pub fn entry_parts(name: &str) -> Result<Vec<String>, String> {
    let refuse = |why: &str| Err(format!("the zip was refused: \"{}\" {why}. Nothing was installed.", name.escape_debug()));
    let b = name.as_bytes();
    if name.contains('\0') {
        return refuse("holds a NUL character");
    }
    if name.starts_with(['/', '\\']) {
        return refuse("is an absolute path");
    }
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return refuse("names a drive");
    }
    let parts: Vec<String> = name.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").map(String::from).collect();
    if parts.iter().any(|p| p == "..") {
        return refuse("leaves its folder (..)");
    }
    Ok(parts)
}

/// Checks the size an entry says it unpacks to, before anything is unpacked, and adds it to the
/// running `total`. The sizes are only claims, so unpacking counts again (a zip bomb lies).
pub fn within_limits(name: &str, size: u64, total: &mut u64) -> Result<(), String> {
    if size > MAX_FILE_BYTES {
        return Err(format!("the zip was refused: {name} unpacks to {} MB, and a file may be at most {} MB. Nothing was installed.", size / (1024 * 1024), MAX_FILE_BYTES / (1024 * 1024)));
    }
    *total = total.saturating_add(size);
    if *total > MAX_TOTAL_BYTES {
        return Err(format!("the zip was refused: its files unpack to more than {} MB. Nothing was installed.", MAX_TOTAL_BYTES / (1024 * 1024)));
    }
    Ok(())
}

/// How many entries a zip's end record says it holds (the zip64 one when the count overflows).
/// A reader that keeps one entry per name sees fewer when a name repeats, and a file named twice
/// can show one thing to one tool and another to Wardian, so the two counts must agree.
pub fn entries_declared(zip: &[u8]) -> Option<u64> {
    const END: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    let last = zip.len().checked_sub(22)?;
    let at = (last.saturating_sub(65_535)..=last).rev().find(|&i| zip[i..i + 4] == END)?;
    let total = u16::from_le_bytes([zip[at + 10], zip[at + 11]]);
    if total != u16::MAX {
        return Some(u64::from(total));
    }
    let locator = zip.get(at.checked_sub(20)?..at)?;
    if locator[..4] != [0x50, 0x4b, 0x06, 0x07] {
        return None;
    }
    let offset = usize::try_from(u64::from_le_bytes(locator[8..16].try_into().ok()?)).ok()?;
    let record = zip.get(offset..offset.checked_add(40)?)?;
    if record[..4] != [0x50, 0x4b, 0x06, 0x06] {
        return None;
    }
    Some(u64::from_le_bytes(record[32..40].try_into().ok()?))
}

/// A zip entry that survived the path checks, split into its folder parts.
pub struct Entry {
    pub index: usize,
    pub parts: Vec<String>,
}

/// Hidden files, macOS's __MACOSX/ copies and build caches are not app files.
pub fn is_app_file(parts: &[String]) -> bool {
    !parts.iter().any(|p| p.starts_with('.') || p == "__MACOSX" || SKIP_DIRS.contains(&p.as_str()))
}

/// One app to install: its name and (zip entry index, path inside the app) for each file.
pub struct Plan {
    pub name: String,
    pub files: Vec<(usize, String)>,
}

pub fn plan(entries: &[Entry], zip_name: &str, skipped: &mut Vec<String>) -> Result<Vec<Plan>, String> {
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
        let top = (0..=dir.len()).rev().find(|&n| has_manifest.contains(&dir[..n])).unwrap_or_else(|| {
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
    // Names are compared without case: on a disk that ignores case, Hello and hello are one folder.
    let mut names = HashSet::new();
    for (name, _) in roots.values() {
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(format!("the zip was refused: it holds two apps named \"{name}\" (names are compared without case). Nothing was installed."));
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
    for plan in plans.values() {
        let mut seen = HashSet::new();
        if let Some((_, rel)) = plan.files.iter().find(|(_, rel)| !seen.insert(rel.to_ascii_lowercase())) {
            return Err(format!("the zip was refused: it holds {}/{rel} twice (names are compared without case). Nothing was installed.", plan.name));
        }
    }
    for (top, (_, main)) in &roots {
        let Some(main) = main else { continue };
        let plan = plans.get_mut(top).expect("an app folder holds at least its module");
        if !plan.files.iter().any(|(_, rel)| rel == "app.wasm") {
            plan.files.push((main.index, "app.wasm".into()));
        }
    }
    let mut out: Vec<Plan> = plans.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_names_refuse_the_zip() {
        assert_eq!(entry_parts("hello/./ui//x.js").unwrap(), ["hello", "ui", "x.js"]);
        assert_eq!(entry_parts("hello\\app.wasm").unwrap(), ["hello", "app.wasm"], "a Windows separator");
        for (name, why) in [
            ("../evil/app.wasm", "leaves its folder"),
            ("hello/../../evil", "leaves its folder"),
            ("hello\\..\\..\\evil", "leaves its folder"),
            ("/etc/hello/app.wasm", "absolute path"),
            ("\\\\server\\share\\x", "absolute path"),
            ("C:/Windows/x", "names a drive"),
            ("hello/a\0b", "NUL"),
        ] {
            let e = entry_parts(name).unwrap_err();
            assert!(e.contains(why) && e.contains("refused"), "{name}: {e}");
        }
    }

    #[test]
    fn declared_sizes_are_limited() {
        let mut total = 0;
        assert!(within_limits("a", MAX_FILE_BYTES, &mut total).is_ok());
        assert!(within_limits("b", MAX_FILE_BYTES + 1, &mut total).unwrap_err().contains("at most 64 MB"));
        let mut total = 0;
        let r: Result<Vec<()>, String> = (0..5).map(|i| within_limits(&i.to_string(), 60 * 1024 * 1024, &mut total)).collect();
        assert!(r.unwrap_err().contains("more than 256 MB"));
        let mut total = u64::MAX - 1;
        assert!(within_limits("c", 10, &mut total).is_err(), "the total cannot wrap around");
    }

    #[test]
    fn the_end_record_gives_the_count() {
        let mut zip = vec![0u8; 30];
        zip.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 3, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(entries_declared(&zip), Some(3));
        assert_eq!(entries_declared(b"not a zip"), None);
    }

    fn entry(index: usize, path: &str) -> Entry {
        Entry { index, parts: path.split('/').map(String::from).collect() }
    }

    #[test]
    fn duplicate_apps_and_files_refuse_the_zip() {
        let mut skipped = Vec::new();
        let two = [entry(0, "a/hello/app.wasm"), entry(1, "b/hello/app.wasm")];
        assert!(plan(&two, "x.zip", &mut skipped).err().unwrap().contains("two apps named"));
        let cased = [entry(0, "hello/app.wasm"), entry(1, "Hello/app.wasm")];
        assert!(plan(&cased, "x.zip", &mut skipped).err().unwrap().contains("two apps named"), "Hello and hello are one folder on macOS");
        let files = [entry(0, "hello/app.wasm"), entry(1, "hello/index.html"), entry(2, "hello/INDEX.html")];
        assert!(plan(&files, "x.zip", &mut skipped).err().unwrap().contains("twice"));
        let fine = [entry(0, "hello/app.wasm"), entry(1, "hello/index.html")];
        assert_eq!(plan(&fine, "x.zip", &mut skipped).unwrap()[0].files.len(), 2);
    }
}
