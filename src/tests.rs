//! Tests that run the use cases against real adapters (the disk, the built-in files), so they
//! belong to the composition root, the one place allowed to know both. Pure rules are tested
//! next to them in the domain.

use crate::adapters::secondary::{embedded_assets::Embedded, local_disk::LocalDisk};
use crate::domain::components::{COMPONENTS, UI_PAGE, UI_SUITE};
use crate::domain::studio::EMPTY_WASM;
use crate::ports::{assets::Assets, storage::FileSystem};
use crate::usecases::{check::Checker, docs::Docs, scaffold::Scaffold};
use serde_json::Value;
use std::{fs, path::Path, sync::Arc};

fn checker() -> Checker {
    Checker::new(Arc::new(LocalDisk))
}

fn scaffold() -> Scaffold {
    Scaffold::new(Arc::new(LocalDisk), Arc::new(Embedded), Arc::new(checker()))
}

fn passes(path: &Path) -> bool {
    checker().check_path(path).0
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("wardian-test-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// ---------- wardian check ----------

#[test]
fn real_packages_pass() {
    assert!(passes(Path::new("apps/usl-lab")));
    assert!(passes(Path::new("tests/fixtures/rogue")));
}

#[test]
fn channels_need_format_2() {
    let dir = tmp("channels");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("app.wasm"), EMPTY_WASM).unwrap();
    fs::write(dir.join("app.json"), r#"{"format":1,"channels":{"send":["budget"]}}"#).unwrap();
    assert!(!checker().check_dir(&dir, "x").0, "format 1 with channels must fail");
    fs::write(dir.join("app.json"), r#"{"format":2,"channels":{"send":["Budget!"]}}"#).unwrap();
    assert!(!checker().check_dir(&dir, "x").0, "a bad channel name must fail");
    fs::write(dir.join("app.json"), r#"{"format":2,"channels":{"send":["budget"],"receive":["loan.v1"]}}"#).unwrap();
    assert!(checker().check_dir(&dir, "x").0);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn broken_suite_fails() {
    assert!(!passes(Path::new("tests/fixtures/broken")));
}

#[test]
fn a_page_app_without_wasm_needs_the_marker() {
    // Make an app writes no app.wasm for a page app; Wardian adds the empty module that marks
    // the folder as an app. Without it the package is not an app.
    let dir = tmp("marker").join("hi");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("app.json"), br#"{"format":1,"title":"Hi","page":"index.html"}"#).unwrap();
    fs::write(dir.join("index.html"), b"<!doctype html><p>hi</p>").unwrap();
    assert!(!checker().check_dir(&dir, "hi").0, "no app.wasm: not a package");
    fs::write(dir.join("app.wasm"), EMPTY_WASM).unwrap();
    let (ok, report) = checker().check_dir(&dir, "hi");
    assert!(ok, "{report}");
    fs::remove_file(dir.join("index.html")).unwrap();
    assert!(!checker().check_dir(&dir, "hi").0, "a missing page must fail");
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

// ---------- wardian new ----------

#[test]
fn every_template_passes_check() {
    let base = tmp("new");
    let tools = scaffold();
    for (kind, _) in crate::domain::components::KINDS {
        let dir = base.join(format!("demo-{kind}"));
        tools.create(kind, &dir).unwrap();
        assert!(passes(&dir), "{kind} template fails wardian check");
        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(readme.contains(&format!("demo-{kind}")) && !readme.contains("{{name}}"));
    }
    assert!(tools.create("page", &base.join("demo-page")).is_err(), "must not overwrite");
    for (kind, ui) in [("page", UI_PAGE), ("suite", UI_SUITE)] {
        for f in ui {
            let got = fs::read_to_string(base.join(format!("demo-{kind}/ui/{f}"))).unwrap();
            assert_eq!(got, Embedded.ui_file(f).unwrap(), "new {kind} package: ui/{f} is not the library's");
        }
    }
    let _ = fs::remove_dir_all(base);
}

/// The ui/ files inside templates/ (and the bundled apps) are copies of the library. If a component
/// changes, they must change with it: run `wardian add --force <component> <package>`.
#[test]
fn ui_copies_match_the_library() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for base in ["templates", "apps"] {
        for pkg in fs::read_dir(root.join(base)).unwrap().flatten() {
            let Ok(rd) = fs::read_dir(pkg.path().join("ui")) else { continue };
            for f in rd.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                let lib = Embedded.ui_file(&name).unwrap_or_else(|| panic!("{}: ui/{name} is not a library file", pkg.path().display()));
                assert_eq!(fs::read_to_string(f.path()).unwrap(), lib, "{}/ui/{name} differs from static/ui/{name}", pkg.path().display());
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "no ui/ copies found");
}

// ---------- wardian add ----------

#[test]
fn every_component_file_exists() {
    for (_, _, files) in COMPONENTS {
        for f in *files {
            assert!(Embedded.ui_file(f).is_some(), "{f} is not built in");
        }
    }
}

#[test]
fn add_copies_and_wires_a_suite() {
    // A bare suite, not `wardian new`: the templates already carry the library.
    let tools = scaffold();
    let dir = tmp("add-suite").join("demo");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("suite.json"), "{\n  \"$schema\": \"http://127.0.0.1:8000/schemas/suite.schema.json\",\n  \"format\": 1,\n  \"title\": \"Demo\",\n  \"styles\": [\"style.css\"],\n  \"apps\": [{ \"name\": \"a\" }]\n}\n").unwrap();
    fs::write(dir.join("style.css"), "").unwrap();
    fs::create_dir_all(dir.join("apps/a")).unwrap();
    fs::write(dir.join("apps/a/app.js"), "Kernel.register({ name: 'a', init() {} });\n").unwrap();
    let a = tools.add(&names(&["button", "tabs", "toast"]), &dir, false).unwrap();
    assert!(a.written.contains(&"ui/theme.css".into()) && a.written.contains(&"ui/tabs.js".into()));
    assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), Embedded.ui_file("button.css").unwrap());
    let s: Value = serde_json::from_str(&fs::read_to_string(dir.join("suite.json")).unwrap()).unwrap();
    let styles: Vec<&str> = s["styles"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(styles, ["ui/theme.css", "ui/button.css", "ui/tabs.css", "ui/toast.css", "style.css"]);
    assert_eq!(s["scripts"], serde_json::json!(["ui/tabs.js", "ui/toast.js"]));
    // suite.json keeps its key order.
    let keys: Vec<&String> = s.as_object().unwrap().keys().collect();
    assert_eq!(keys[..3], ["$schema", "format", "title"]);
    assert!(passes(&dir), "the suite fails wardian check after add");

    // A second add keeps the author's copies and wires nothing twice.
    fs::write(dir.join("ui/button.css"), "/* mine */").unwrap();
    let b = tools.add(&names(&["button", "dialog"]), &dir, false).unwrap();
    assert!(b.skipped.contains(&"ui/button.css".into()) && b.skipped.contains(&"ui/theme.css".into()));
    assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), "/* mine */");
    assert_eq!(b.wired, ["styles: ui/dialog.css", "scripts: ui/dialog.js"]);
    let s: Value = serde_json::from_str(&fs::read_to_string(dir.join("suite.json")).unwrap()).unwrap();
    assert_eq!(s["styles"].as_array().unwrap().iter().filter(|v| v == &"ui/button.css").count(), 1);
    // --force replaces it.
    tools.add(&names(&["button"]), &dir, true).unwrap();
    assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), Embedded.ui_file("button.css").unwrap());
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn add_to_a_page_gives_tags_and_refuses_modules_and_unknowns() {
    let tools = scaffold();
    let base = tmp("add-page");
    let page = base.join("pg");
    tools.create("page", &page).unwrap();
    let a = tools.add(&names(&["dialog"]), &page, false).unwrap();
    assert_eq!(a.page_tags, ["<link rel=\"stylesheet\" href=\"ui/theme.css\">", "<link rel=\"stylesheet\" href=\"ui/dialog.css\">", "<script src=\"ui/dialog.js\"></script>"]);
    assert!(a.wired.is_empty());
    let module = base.join("md");
    tools.create("module", &module).unwrap();
    assert!(tools.add(&names(&["button"]), &module, false).unwrap_err().contains("module app"));
    assert!(tools.add(&names(&["sparkles"]), &page, false).unwrap_err().contains("unknown component"));
    let _ = fs::remove_dir_all(base);
}

// ---------- docs ----------

#[test]
fn docs_render_with_anchors() {
    let docs = Docs::new(Arc::new(Embedded));
    let spec = docs.page("spec").unwrap();
    assert!(spec.contains("id=\"6-5-ctx\"") && spec.contains("<table>"));
    assert!(docs.page("guide").unwrap().contains("wardian new module"));
    assert!(docs.page("nope").is_none());
    assert!(docs.schema("app.schema.json").is_some() && docs.schema("x.json").is_none());
}

// ---------- the files port ----------

#[test]
fn private_files_are_written_whole() {
    let dir = tmp("private");
    let disk = LocalDisk;
    let path = dir.join("a/b/key");
    disk.write_private(&path, b"secret").unwrap();
    assert_eq!(disk.read(&path).unwrap(), b"secret");
    assert!(!disk.exists(&path.with_extension("tmp")), "the temp file is renamed away");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let _ = fs::remove_dir_all(dir);
}

// ---------- the viewer's state (ADR-2610071055) ----------

#[test]
fn viewer_state_is_kept_private_on_disk() {
    use crate::ports::service::ViewerState;
    use crate::usecases::viewer_state::State;
    use serde_json::json;
    let dir = tmp("state");
    let state = State::new(Arc::new(LocalDisk), &dir);

    // Layouts: kept, read back, forgotten.
    assert_eq!(state.layout("usl-lab").unwrap(), Value::Null);
    state.set_layout("usl-lab", json!({"v": 1, "hidden": ["meaning"]})).unwrap();
    assert_eq!(state.layout("usl-lab").unwrap()["hidden"][0], "meaning");
    assert!(state.set_layout("../evil", json!({})).is_err(), "names stay inside the state folder");

    // App data: one key at a time, and a browser's data merged without overwriting the host's.
    state.set_app_value("usl-lab", "inputs", "state", json!({"d": "1,1000"})).unwrap();
    let merged = state.merge_app_data("usl-lab", &json!({"inputs": {"state": "old", "tableLink": true}})).unwrap();
    assert_eq!(merged["added"], 1);
    let data = state.app_data("usl-lab").unwrap();
    assert_eq!(data["inputs"]["state"]["d"], "1,1000");
    assert_eq!(data["inputs"]["tableLink"], true);

    // Channels: the latest message.
    state.set_channel("splunk.table", json!({"rows": [[1, 2]]})).unwrap();
    assert_eq!(state.channel("splunk.table")["rows"][0][1], 2);

    // Every file is private, like the keys.
    #[cfg(unix)]
    for f in ["state/layouts.json", "state/apps/usl-lab.json", "state/channels.json"] {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(dir.join(f)).unwrap().permissions().mode() & 0o777, 0o600, "{f}");
    }
    let _ = fs::remove_dir_all(dir);
}

// ---------- the working folder and each app's history (ADR-2610071122) ----------

#[test]
fn seeding_history_and_promote() {
    use crate::ports::service::AppHistory;
    use crate::ports::tools::PackageTools;
    use crate::usecases::{history::History, workspace};
    let dir = tmp("history");
    let disk = LocalDisk;
    let (source, data) = (dir.join("src-apps"), dir.join("data"));
    let working = data.join("apps");
    let write = |p: &Path, s: &str| disk.write(p, s.as_bytes()).unwrap();
    write(&source.join("hello/app.json"), r#"{"format":1,"title":"Hello","page":"index.html"}"#);
    write(&source.join("hello/app.wasm"), "\0asm\x01\0\0\0");
    write(&source.join("hello/index.html"), "<p>one</p>\n");
    write(&source.join("hello/target/big.bin"), "build output");
    write(&source.join("hello/Cargo.lock"), "# build output too");
    write(&source.join(".trash/old--1/app.wasm"), "\0asm\x01\0\0\0");

    // Seeding copies the apps and the trash, leaves build output and the source alone.
    assert_eq!(workspace::seed(&disk, &source, &working).unwrap(), Some(1));
    assert!(disk.is_file(&working.join("hello/index.html")) && disk.is_file(&working.join(".trash/old--1/app.wasm")));
    assert!(!disk.exists(&working.join("hello/target")) && !disk.exists(&working.join("hello/Cargo.lock")), "build output is not copied");
    assert_eq!(workspace::seed(&disk, &source, &working).unwrap(), None, "a filled working folder is left as it is");

    // The first change keeps the original as version 1; saves add versions; a restore is a version.
    let history = History::new(Arc::new(LocalDisk), &data, &working);
    assert_eq!(history.before_change("hello"), Some(1));
    write(&working.join("hello/index.html"), "<p>two</p>\n");
    assert_eq!(history.record("hello", "make-an-app", "says two"), Some(2));
    let v = history.versions("hello").unwrap();
    assert_eq!(v["versions"][0]["n"], 2);
    assert_eq!(v["versions"][0]["current"], true);
    assert_eq!(v["versions"][1]["by"], "first-seen");
    let d = history.diff("hello", 1).unwrap();
    assert_eq!(d["files"][0]["path"], "index.html");
    assert_eq!(d["files"][0]["diff"], "-<p>one</p>\n+<p>two</p>\n");
    let r = history.restore("hello", 1).unwrap();
    assert_eq!(r["version"], 3);
    assert_eq!(disk.read(&working.join("hello/index.html")).unwrap(), b"<p>one</p>\n");
    assert_eq!(history.versions("hello").unwrap()["versions"][0]["why"], "restored version 1");
    assert!(history.restore("hello", 99).is_err());
    assert!(history.versions("../x").is_err());

    // Promote copies the working app back into the source, and says what changed.
    write(&working.join("hello/index.html"), "<p>three</p>\n");
    write(&working.join("hello/extra.js"), "// new\n");
    let p = scaffold().promote("hello", &data, &source).unwrap();
    assert_eq!((p.added.clone(), p.changed.clone()), (vec!["extra.js".to_string()], vec!["index.html".to_string()]));
    assert_eq!(disk.read(&source.join("hello/index.html")).unwrap(), b"<p>three</p>\n");
    assert!(disk.exists(&source.join("hello/target/big.bin")), "promote leaves the source's build output alone");
    assert_eq!(history.versions("hello").unwrap()["versions"][0]["by"], "promote");
    let _ = fs::remove_dir_all(dir);
}
