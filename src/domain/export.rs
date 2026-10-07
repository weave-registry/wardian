//! An app as a `.wardian` file (ADR-2610071248): a zip with one package folder at its top, the
//! files `wardian check` reads, and a manifest saying what is inside. With the user's consent it
//! also carries the app's own data. Keys, accounts, permission answers and history never go in.

use super::history::kept_in_copies;
use super::package::safe_segment;
use serde_json::{json, Value};

/// The file's media type, and its type identifier on macOS (`wardian.studio`, reversed).
pub const MIME: &str = "application/vnd.wardian+zip";
const TYPE_ID: &str = "studio.wardian.package";
/// Inside the package folder; a hidden folder, so hosts that predate exports ignore it.
pub const MANIFEST: &str = ".wardian/export.json";
pub const DATA: &str = ".wardian/data";
pub const STORAGE_FILE: &str = "storage.json";
pub const LAYOUT_FILE: &str = "layout.json";
pub const TABLES_FILE: &str = "tables.sqlite";
/// The first bytes of every SQLite database.
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";
/// The largest export, data included.
pub const MAX_EXPORT_BYTES: u64 = 500 * 1024 * 1024;
const FORMAT: u64 = 1;

/// What never leaves with an app, said to the user before a download.
pub const NEVER_INCLUDED: &[&str] = &[
    "keys and accounts (Anthropic, Bedrock, Splunk, Google)",
    "permission answers",
    "history and the trash",
    "other apps' data and tables",
];

/// Whether a file of the app goes into the export: exactly what `wardian check` reads. Hidden
/// files (a `.wardian` folder of an earlier export included), build output and repositories stay out.
pub fn exported(rel: &str) -> bool {
    kept_in_copies(rel) && !rel.split('/').any(|p| p.starts_with('.') || p == "__MACOSX")
}

/// What data an export carries, for the manifest and the preview.
#[derive(Default)]
pub struct DataIncluded {
    pub storage: bool,
    pub layout: bool,
    /// (table, rows)
    pub tables: Vec<(String, u64)>,
}

impl DataIncluded {
    pub fn any(&self) -> bool {
        self.storage || self.layout || !self.tables.is_empty()
    }
    pub fn to_json(&self) -> Value {
        json!({
            "storage": self.storage,
            "layout": self.layout,
            "tables": self.tables.iter().map(|(t, n)| json!({ "name": t, "rows": n })).collect::<Vec<_>>(),
        })
    }
}

/// The manifest at `<package>/.wardian/export.json`.
pub fn manifest(package: &str, title: Option<&str>, at: u64, version: &str, data: &DataIncluded) -> Value {
    json!({
        "format": FORMAT,
        "type": TYPE_ID,
        "package": package,
        "title": title,
        "exported_at": at,
        "wardian_version": version,
        "includes": { "app": true, "data": data.any() },
        "data": data.to_json(),
    })
}

/// A manifest read back from a file: only what a host can act on, checked.
pub struct Manifest {
    pub package: String,
    pub data: bool,
    pub value: Value,
}

pub fn read_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("the file's manifest is not valid JSON: {e}"))?;
    let format = v["format"].as_u64().unwrap_or(0);
    if format == 0 || format > FORMAT {
        return Err(format!("the file was exported in format {format}; this Wardian reads format {FORMAT}"));
    }
    let package = v["package"].as_str().unwrap_or("").to_string();
    if !safe_segment(&package) {
        return Err("the file's manifest names no valid app".into());
    }
    let data = v["includes"]["data"].as_bool().unwrap_or(false);
    Ok(Manifest { package, data, value: v })
}

/// Whether a file's first bytes are an SQLite database's: what an imported `tables.sqlite` must be
/// before Wardian hands it to SQLite.
pub fn looks_like_sqlite(head: &[u8]) -> bool {
    head.starts_with(SQLITE_HEADER)
}

/// The download's file name.
pub fn file_name(package: &str) -> String {
    format!("{package}.wardian")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_what_check_reads() {
        assert!(exported("app.json") && exported("apps/chart/app.js") && exported("ui/theme.css"));
        for left_out in [".wardian/export.json", ".git/HEAD", "target/release/x", "web/node_modules/a.js", ".DS_Store", "Cargo.lock", "__MACOSX/x"] {
            assert!(!exported(left_out), "{left_out}");
        }
    }

    #[test]
    fn manifests_round_trip_and_are_checked() {
        let data = DataIncluded { storage: true, layout: false, tables: vec![("search_1".into(), 120)] };
        let m = manifest("usl-lab", Some("USL lab"), 1, "0.4.0", &data);
        let back = read_manifest(m.to_string().as_bytes()).unwrap();
        assert_eq!(back.package, "usl-lab");
        assert!(back.data);
        assert_eq!(back.value["data"]["tables"][0]["rows"], 120);
        assert!(read_manifest(br#"{"format":9,"package":"x"}"#).is_err(), "a newer format is refused");
        assert!(read_manifest(br#"{"format":1,"package":"../x"}"#).is_err(), "the name stays a name");
        assert!(!read_manifest(br#"{"format":1,"package":"x"}"#).unwrap().data, "no data unless it says so");
        assert!(looks_like_sqlite(b"SQLite format 3\0\x10\0") && !looks_like_sqlite(b"SQLite format 3") && !looks_like_sqlite(b"PK\x03\x04"));
    }
}
