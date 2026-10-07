//! An app as a `.wardian` file (ADR-2610071248). Export writes the files `wardian check` reads and a
//! manifest, and on request the app's data: its `storage` data, its Arrange layout and a consistent
//! copy of its SQLite tables. An import shows what a file holds before anything is installed, and
//! installs its data only when asked, keeping the data it replaces next to the app's latest version.

use super::check::Checker;
use super::history::History;
use crate::domain::export::{
    exported, file_name, looks_like_sqlite, manifest, read_manifest, DataIncluded, Manifest, DATA, LAYOUT_FILE, MANIFEST, MAX_EXPORT_BYTES, MIME, NEVER_INCLUDED, STORAGE_FILE,
    TABLES_FILE,
};
use crate::domain::package::{app_info, safe_segment, unix_now, APP_MARKERS};
use crate::ports::{
    db::Database,
    service::{Exports, ViewerState},
    storage::FileSystem,
};
use serde_json::{json, Value};
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// The largest single entry read back from a file on import.
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

pub struct Exporter {
    fs: Arc<dyn FileSystem>,
    checker: Arc<Checker>,
    db: Arc<dyn Database>,
    state: Arc<dyn ViewerState>,
    history: Arc<History>,
    apps: PathBuf,
    data_dir: PathBuf,
}

/// A file opened for import: the zip, and its manifest with the folder it sits in, if it has one.
type Opened<'a> = (zip::ZipArchive<Cursor<&'a [u8]>>, Option<(String, Manifest)>);

/// The app's data, ready to write: what goes in, and the bytes of each file.
struct Data {
    included: DataIncluded,
    files: Vec<(&'static str, Vec<u8>)>,
}

impl Exporter {
    pub fn new(fs: Arc<dyn FileSystem>, checker: Arc<Checker>, db: Arc<dyn Database>, state: Arc<dyn ViewerState>, history: Arc<History>, apps: &Path, data_dir: &Path) -> Exporter {
        Exporter { fs, checker, db, state, history, apps: apps.to_path_buf(), data_dir: data_dir.to_path_buf() }
    }

    fn app_dir(&self, app: &str) -> Result<PathBuf, String> {
        if !safe_segment(app) {
            return Err(format!("\"{app}\" is not an app name"));
        }
        let dir = self.apps.join(app);
        if !APP_MARKERS.iter().any(|m| self.fs.is_file(&dir.join(m))) {
            return Err(format!("no app \"{app}\" in the local apps folder"));
        }
        Ok(dir)
    }

    /// The app's files that go in, with their sizes.
    fn files(&self, dir: &Path) -> Vec<(String, u64)> {
        let mut files: Vec<(String, u64)> = self.fs.walk(dir).into_iter().filter(|(rel, _)| exported(rel)).collect();
        files.sort();
        files
    }

    /// The app's data, read now. With `bytes` false only the summary is built (for a preview).
    fn data(&self, app: &str, bytes: bool) -> Result<Data, String> {
        let mut included = DataIncluded::default();
        let mut files = Vec::new();
        let storage = self.state.app_data(app)?;
        if storage.as_object().is_some_and(|o| !o.is_empty()) {
            included.storage = true;
            files.push((STORAGE_FILE, serde_json::to_vec_pretty(&storage).map_err(|e| e.to_string())?));
        }
        let layout = self.state.layout(app)?;
        if !layout.is_null() {
            included.layout = true;
            files.push((LAYOUT_FILE, serde_json::to_vec_pretty(&layout).map_err(|e| e.to_string())?));
        }
        let tables = self.db.tables(app).unwrap_or(Value::Array(vec![]));
        for t in tables.as_array().into_iter().flatten() {
            included.tables.push((t["name"].as_str().unwrap_or("").to_string(), t["rows"].as_u64().unwrap_or(0)));
        }
        if bytes && !included.tables.is_empty() {
            let tmp = self.fs.temp_path("wardian-export");
            self.fs.create_dir_all(&tmp)?;
            let copy = tmp.join(TABLES_FILE);
            let result = self.db.backup(app, &copy).map(|made| if made { self.fs.read(&copy) } else { None });
            self.fs.remove_dir_all(&tmp);
            if let Some(b) = result? {
                files.push((TABLES_FILE, b));
            }
        }
        Ok(Data { included, files })
    }

    /// Opens a `.wardian` or `.zip` file, finds its manifest, and the folder it sits in. A file
    /// with more than one manifest, or whose manifest names another folder than its own, is
    /// refused: its data could otherwise land in an app it does not belong to.
    fn open(bytes: &[u8]) -> Result<Opened<'_>, String> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a valid zip: {e}"))?;
        let all: Vec<String> = zip.file_names().filter(|n| n.ends_with(&format!("/{MANIFEST}")) && n.matches('/').count() == 2).map(String::from).collect();
        if all.len() > 1 {
            return Err("the file was refused: it holds more than one exported app; a .wardian file holds one".into());
        }
        let found = match all.into_iter().next() {
            Some(name) => {
                let raw = read_entry(&mut zip, &name)?.ok_or("the manifest could not be read")?;
                let folder = name.trim_end_matches(MANIFEST).trim_end_matches('/').to_string();
                let man = read_manifest(&raw)?;
                if folder != man.package {
                    return Err(format!("the file was refused: its manifest names the app \"{}\" but sits in the folder \"{}\"", man.package, folder.escape_debug()));
                }
                if man.data {
                    check_data(&mut zip, &folder)?;
                }
                Some((folder, man))
            }
            None => None,
        };
        Ok((zip, found))
    }
}

/// Checks the data a file carries before any of it is read or installed: each file within the
/// size limit by what it says and a plain file, and the tables an SQLite database by their first
/// bytes. A file that fails is refused whole, so an import never installs half its data.
fn check_data(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, folder: &str) -> Result<(), String> {
    for name in [STORAGE_FILE, LAYOUT_FILE, TABLES_FILE] {
        let path = format!("{folder}/{DATA}/{name}");
        let Ok(f) = zip.by_name(&path) else { continue };
        if !f.is_file() {
            return Err(format!("the file was refused: its {name} is not a plain file"));
        }
        if f.size() > MAX_ENTRY_BYTES {
            return Err(format!("the file was refused: its {name} unpacks to {} MB, and data files may be at most {} MB", f.size() / (1024 * 1024), MAX_ENTRY_BYTES / (1024 * 1024)));
        }
        if name == TABLES_FILE {
            let mut head = Vec::new();
            f.take(16).read_to_end(&mut head).map_err(|e| format!("reading {path}: {e}"))?;
            if !looks_like_sqlite(&head) {
                return Err(format!("the file was refused: its {TABLES_FILE} is not an SQLite database"));
            }
        }
    }
    Ok(())
}

fn read_entry(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let Ok(f) = zip.by_name(name) else { return Ok(None) };
    let mut buf = Vec::new();
    f.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).map_err(|e| format!("reading {name}: {e}"))?;
    if buf.len() as u64 > MAX_ENTRY_BYTES {
        return Err(format!("the file was refused: {name} unpacks to more than {} MB", MAX_ENTRY_BYTES / (1024 * 1024)));
    }
    Ok(Some(buf))
}

impl Exports for Exporter {
    fn preview(&self, app: &str, with_data: bool) -> Result<Value, String> {
        let dir = self.app_dir(app)?;
        let files = self.files(&dir);
        let size: u64 = files.iter().map(|(_, s)| s).sum();
        let data = if with_data { Some(self.data(app, false)?) } else { None };
        Ok(json!({
            "app": app,
            "file": file_name(app),
            "type": MIME,
            "files": files.iter().map(|(p, s)| json!({ "path": p, "size": s })).collect::<Vec<_>>(),
            "bytes": size,
            "data": data.map(|d| d.included.to_json()),
            "never": NEVER_INCLUDED,
        }))
    }

    fn export(&self, app: &str, with_data: bool) -> Result<Vec<u8>, String> {
        let dir = self.app_dir(app)?;
        let (ok, report) = self.checker.check_dir(&dir, app);
        if !ok {
            return Err(format!("{app} does not pass `wardian check`, so it cannot be exported:\n{report}"));
        }
        let files = self.files(&dir);
        let data = if with_data { self.data(app, true)? } else { Data { included: DataIncluded::default(), files: vec![] } };
        let total: u64 = files.iter().map(|(_, s)| s).sum::<u64>() + data.files.iter().map(|(_, b)| b.len() as u64).sum::<u64>();
        if total > MAX_EXPORT_BYTES {
            return Err(format!("{app} with its data is larger than {} MB, the most one file may hold", MAX_EXPORT_BYTES / (1024 * 1024)));
        }
        let info = app_info(app.to_string(), &|rel| self.fs.read(&dir.join(rel)), &|rel| self.fs.is_file(&dir.join(rel)));
        let man = manifest(app, info.title.as_deref(), unix_now(), env!("CARGO_PKG_VERSION"), &data.included);

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: String, bytes: &[u8]| -> Result<(), String> {
            zip.start_file(name.as_str(), opts).map_err(|e| format!("writing {name}: {e}"))?;
            zip.write_all(bytes).map_err(|e| format!("writing {name}: {e}"))
        };
        for (rel, _) in &files {
            let bytes = self.fs.read(&dir.join(rel)).ok_or_else(|| format!("cannot read {app}/{rel}"))?;
            put(format!("{app}/{rel}"), &bytes)?;
        }
        for (name, bytes) in &data.files {
            put(format!("{app}/{DATA}/{name}"), bytes)?;
        }
        put(format!("{app}/{MANIFEST}"), serde_json::to_string_pretty(&man).map_err(|e| e.to_string())?.as_bytes())?;
        let out = zip.finish().map_err(|e| format!("finishing the file: {e}"))?;
        println!("export: {app}{}", if data.included.any() { " with its data" } else { "" });
        Ok(out.into_inner())
    }

    fn preview_import(&self, bytes: &[u8]) -> Result<Value, String> {
        let (mut zip, found) = Exporter::open(bytes)?;
        let Some((folder, man)) = found else {
            return Ok(json!({ "manifest": null, "data": null, "note": "an app without a manifest: only its files are installed" }));
        };
        // What the app may use, read from the file as "Sealed" shows it for an installed app.
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        let mut read = |rel: &str| read_entry(&mut zip, &format!("{folder}/{rel}")).ok().flatten();
        let app_json = read("app.json");
        let suite_json = read("suite.json");
        let has = |rel: &str| names.iter().any(|n| n == &format!("{folder}/{rel}"));
        let info = app_info(man.package.clone(), &|rel| match rel {
            "app.json" => app_json.clone(),
            "suite.json" => suite_json.clone(),
            _ => None,
        }, &has);
        Ok(json!({
            "manifest": man.value,
            "package": man.package,
            "title": info.title,
            "suite": info.suite,
            "allows": info.allows,
            "data": if man.data { man.value["data"].clone() } else { Value::Null },
        }))
    }

    fn keep_data_before_import(&self, app: &str) -> Result<(), String> {
        if self.app_dir(app).is_err() {
            return Ok(()); // a new app has no data to keep
        }
        let Some(n) = self.history.before_change(app) else { return Ok(()) };
        let data = self.data(app, true)?;
        if !data.included.any() {
            return Ok(());
        }
        // Next to the version it belongs to; restoring a version puts back its files, and these
        // are there to be put back by hand (ADR-2610071248).
        let dir = self.data_dir.join("history").join(app).join(format!("{n}.data"));
        for (name, bytes) in &data.files {
            self.fs.write_private(&dir.join(name), bytes)?;
        }
        Ok(())
    }

    fn install_data(&self, app: &str, bytes: &[u8]) -> Result<Value, String> {
        self.app_dir(app)?;
        let (mut zip, found) = Exporter::open(bytes)?;
        let Some((folder, man)) = found.filter(|(_, m)| m.data) else {
            return Ok(json!({ "installed": false, "note": "the file holds no data" }));
        };
        // Everything is read and checked first, and the tables, the part most likely to be
        // refused, go in first, so a refused file leaves the app's data as it was.
        let json = |b: Option<Vec<u8>>, what: &str| -> Result<Option<Value>, String> {
            b.map(|b| serde_json::from_slice(&b).map_err(|e| format!("the file's {what} is not valid JSON: {e}"))).transpose()
        };
        let storage = json(read_entry(&mut zip, &format!("{folder}/{DATA}/{STORAGE_FILE}"))?, "storage data")?;
        let layout = json(read_entry(&mut zip, &format!("{folder}/{DATA}/{LAYOUT_FILE}"))?, "layout")?;
        let mut installed = Vec::new();
        if let Some(b) = read_entry(&mut zip, &format!("{folder}/{DATA}/{TABLES_FILE}"))? {
            let tmp = self.fs.temp_path("wardian-import");
            self.fs.create_dir_all(&tmp)?;
            let file = tmp.join(TABLES_FILE);
            let result = self.fs.write(&file, &b).and_then(|()| self.db.restore(app, &file));
            self.fs.remove_dir_all(&tmp);
            result.map_err(|e| format!("the file's tables were not installed: {e}"))?;
            installed.push("tables");
        }
        if let Some(v) = storage {
            self.state.replace_app_data(app, &v)?;
            installed.push("storage");
        }
        if let Some(v) = layout {
            self.state.set_layout(app, v)?;
            installed.push("layout");
        }
        println!("import: {app}: installed its data ({}) from the file of {}", installed.join(", "), man.package);
        Ok(json!({ "installed": true, "what": installed }))
    }
}
