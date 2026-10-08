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
    // The rogue suite breaks two rules on purpose, for the kernel tests: `closer`'s app.js contains
    // `</script` (SPEC 6.3), and `probe` needs a method `liar` does not provide (SPEC 6.7.4).
    // Check must report both, and nothing else may fail.
    let (ok, report) = checker().check_path(Path::new("tests/fixtures/rogue"));
    let errors: Vec<&str> = report.iter().flat_map(|b| b.lines()).filter(|l| l.trim_start().starts_with("error")).collect();
    assert!(!ok && errors.len() == 2, "{report:?}");
    assert!(errors.iter().any(|l| l.contains("closer")) && errors.iter().any(|l| l.contains("liar.nothing")), "{report:?}");
}

/// ADR-2610080900: no example part's app.js is over 250 lines.
#[test]
fn claim_example_parts_stay_small() {
    let mut big = Vec::new();
    for pkg in fs::read_dir("apps").unwrap().flatten() {
        let Ok(parts) = fs::read_dir(pkg.path().join("apps")) else { continue };
        for part in parts.flatten() {
            let js = part.path().join("app.js");
            if let Ok(text) = fs::read_to_string(&js) {
                let lines = text.lines().count();
                if lines > 250 {
                    big.push(format!("{} ({lines} lines)", js.display()));
                }
            }
        }
    }
    assert!(big.is_empty(), "parts over 250 lines: {}", big.join(", "));
}

/// ADR-2610080900: the Splunk table is made of small parts, so check has nothing to say about its size.
#[test]
fn splunk_table_parts_pass_check_without_a_split_warning() {
    let (ok, report) = checker().check_path(Path::new("apps/splunk-table"));
    assert!(ok, "{report:?}");
    assert!(!report.join("").contains("ADR-2610080900"), "{report:?}");
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
    assert!(docs.page("index").unwrap().contains("Where to start"), "the home page is a page");
}

/// Every `/docs/<name>#<id>` link on every page names a page that exists, and a heading on it.
#[test]
fn docs_links_resolve() {
    let docs = Docs::new(Arc::new(Embedded));
    let names: Vec<&str> = Embedded.docs().iter().map(|p| p.name).collect();
    let mut seen = std::collections::HashSet::new();
    assert!(names.iter().all(|n| seen.insert(*n)), "two docs pages share a name");
    let pages: std::collections::HashMap<&str, String> = names.iter().map(|n| (*n, docs.page(n).unwrap())).collect();
    let mut broken = Vec::new();
    for (from, html) in &pages {
        for link in html.split("href=\"/docs/").skip(1) {
            let target = &link[..link.find('"').unwrap()];
            let (name, anchor) = target.split_once('#').unwrap_or((target, ""));
            let name = if name.is_empty() { "index" } else { name };
            match pages.get(name) {
                None => broken.push(format!("{from}: /docs/{target} (no such page)")),
                Some(to) if !anchor.is_empty() && !to.contains(&format!("id=\"{anchor}\"")) => {
                    broken.push(format!("{from}: /docs/{target} (no such heading)"))
                }
                _ => {}
            }
        }
    }
    assert!(broken.is_empty(), "broken docs links:\n{}", broken.join("\n"));
}

/// website/ holds the docs site as `wardian docs website` writes it, so the website never shows
/// other text than Wardian does (ADR-2610080903).
#[test]
fn docs_website_copy_is_fresh() {
    let docs = Docs::new(Arc::new(Embedded));
    let stale: Vec<String> = docs
        .site()
        .into_iter()
        .filter(|(rel, body)| fs::read_to_string(Path::new("website").join(rel)).ok().as_deref() != Some(body.as_str()))
        .map(|(rel, _)| rel)
        .collect();
    assert!(stale.is_empty(), "website/ is out of date; run `wardian docs website` and commit. Stale: {}", stale.join(", "));
}

// ---------- skills ----------

/// Every shipped skill has a SKILL.md named after its folder, with a description, and carries
/// every reference page it asks for (ADR-2610080928).
#[test]
fn skills_are_whole() {
    let files = crate::usecases::skills::files(&Embedded);
    let names = crate::usecases::skills::names(&Embedded);
    assert_eq!(names, ["wardian-app-factory", "wardian-app-doctor"]);
    for n in &names {
        let skill = files.iter().find(|(p, _)| *p == format!("{n}/SKILL.md")).unwrap_or_else(|| panic!("{n} has no SKILL.md")).1.clone();
        assert!(skill.starts_with("---\n") && skill.contains(&format!("\nname: {n}\n")) && skill.contains("\ndescription: "), "{n}: SKILL.md front matter");
        for r in ["spec", "ctx", "capabilities"] {
            assert!(files.iter().any(|(p, _)| *p == format!("{n}/references/{r}.md")), "{n} lacks references/{r}.md");
        }
        for (p, body) in files.iter().filter(|(p, _)| p.starts_with(&format!("{n}/"))) {
            for r in body.split("references/").skip(1).filter_map(|t| t.split('`').next()).filter(|t| t.ends_with(".md")) {
                assert!(files.iter().any(|(q, _)| *q == format!("{n}/references/{r}")), "{p} names references/{r}, which {n} does not carry");
            }
        }
    }
}

/// A reference page that is not a docs page stops the install, and is not dropped without a word.
/// "security" is one only wardian-app-doctor carries, which its SKILL.md names as a bare `security.md`.
#[test]
#[should_panic(expected = "wardian-app-doctor carries references/security.md, but there is no docs page security")]
fn skills_refuse_a_missing_reference() {
    struct WithoutSecurity;
    impl Assets for WithoutSecurity {
        fn ui_file(&self, name: &str) -> Option<&'static str> {
            Embedded.ui_file(name)
        }
        fn ui_names(&self) -> Vec<&'static str> {
            Embedded.ui_names()
        }
        fn gallery(&self) -> &'static str {
            Embedded.gallery()
        }
        fn template(&self, kind: &str) -> Option<crate::ports::assets::TemplateFiles> {
            Embedded.template(kind)
        }
        fn frame_shim(&self) -> &'static str {
            Embedded.frame_shim()
        }
        fn spec_md(&self) -> &'static str {
            Embedded.spec_md()
        }
        fn docs(&self) -> &'static [crate::ports::assets::DocPage] {
            let pages = Embedded.docs().iter().filter(|p| p.name != "security");
            let pages = pages.map(|p| crate::ports::assets::DocPage { group: p.group, name: p.name, title: p.title, source: p.source, md: p.md });
            Box::leak(pages.collect::<Vec<_>>().into_boxed_slice())
        }
        fn skills(&self) -> &'static [(&'static str, &'static str)] {
            Embedded.skills()
        }
        fn example_suite(&self) -> &'static [(&'static str, &'static str)] {
            Embedded.example_suite()
        }
        fn schema(&self, name: &str) -> Option<&'static str> {
            Embedded.schema(name)
        }
    }
    crate::usecases::skills::files(&WithoutSecurity);
}

/// This repository uses the skills it ships: .claude/skills/ is what `wardian skills .` writes.
#[test]
fn skills_repo_copy_is_fresh() {
    let stale: Vec<String> = crate::usecases::skills::files(&Embedded)
        .into_iter()
        .filter(|(rel, body)| fs::read_to_string(Path::new(".claude/skills").join(rel)).ok().as_deref() != Some(body.as_str()))
        .map(|(rel, _)| rel)
        .collect();
    assert!(stale.is_empty(), ".claude/skills/ is out of date; run `wardian skills . --force` and commit. Stale: {}", stale.join(", "));
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

// ---------- the first-run setup (ADR-2610072033) ----------

#[test]
fn first_run_marker_only_on_an_empty_data_folder() {
    use crate::usecases::workspace::{mark_first_run, FIRST_RUN_MARKER};
    let dir = tmp("first-run");
    let disk = LocalDisk;
    let data = dir.join("data");
    assert!(mark_first_run(&disk, &data).unwrap(), "a missing data folder is a first start");
    assert!(disk.is_file(&data.join(FIRST_RUN_MARKER)));
    disk.remove_file(&data.join(FIRST_RUN_MARKER));
    disk.write(&data.join("grants.json"), b"[]").unwrap();
    assert!(!mark_first_run(&disk, &data).unwrap(), "a used data folder is not");
    assert!(!disk.exists(&data.join(FIRST_RUN_MARKER)), "and the setup is not shown again");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn data_folder_is_checked_writable_at_start() {
    use crate::usecases::workspace::check_writable;
    let dir = tmp("writable");
    let disk = LocalDisk;
    let data = dir.join("data");
    check_writable(&disk, &data).expect("a missing data folder is made and written");
    assert!(disk.list_dir(&data).is_empty(), "the test file is removed again");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = dir.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        assert!(check_writable(&disk, &locked).is_err(), "a folder Wardian cannot write is refused at start");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn seeding_adds_new_examples_but_not_removed_ones() {
    use crate::usecases::workspace::seed;
    let dir = tmp("seed-new");
    let disk = LocalDisk;
    let (source, working) = (dir.join("examples"), dir.join("apps"));
    let app = |root: &Path, name: &str| {
        disk.write(&root.join(name).join("app.json"), br#"{"format":1,"title":"T","page":"index.html"}"#).unwrap();
        disk.write(&root.join(name).join("index.html"), b"<p>hi</p>").unwrap();
        disk.write(&root.join(name).join("app.wasm"), b"\0asm\x01\0\0\0").unwrap();
    };
    app(&source, "one");
    app(&source, "two");
    assert_eq!(seed(&disk, &source, &working).unwrap(), Some(2), "an empty folder gets every example");
    // A later version ships three more; the user removed "two" to the trash and deleted "gone" for good.
    app(&source, "three");
    app(&source, "four");
    app(&source, "gone");
    disk.write(&working.join(".examples-seen"), b"one\ntwo\ngone\n").unwrap();
    fs::create_dir_all(working.join(".trash")).unwrap();
    fs::rename(working.join("two"), working.join(".trash/two--1791381188")).unwrap();
    assert_eq!(seed(&disk, &source, &working).unwrap(), Some(2), "the two new examples are added");
    assert!(disk.is_file(&working.join("three/app.wasm")) && disk.is_file(&working.join("four/app.wasm")));
    assert!(!disk.exists(&working.join("two")), "an example in the trash stays removed");
    assert!(!disk.exists(&working.join("gone")), "an example offered before and deleted stays deleted");
    assert_eq!(seed(&disk, &source, &working).unwrap(), None, "nothing new, nothing added");
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

// ---------- the db capability (ADR-2610071219) ----------

/// A Splunk that has finished one job with `rows` results, and serves them by offset and count.
struct FakeSplunk {
    rows: usize,
    asked: std::sync::Mutex<Vec<(usize, usize)>>,
}

struct FakeSession(Arc<FakeSplunk>);

impl crate::ports::splunk::SplunkApi for Arc<FakeSplunk> {
    fn connect(&self, _: &crate::ports::splunk::SplunkConfig) -> Result<Box<dyn crate::ports::splunk::SplunkSession>, String> {
        Ok(Box::new(FakeSession(Arc::clone(self))))
    }
}

impl crate::ports::splunk::SplunkSession for FakeSession {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, crate::ports::splunk::SplunkError> {
        use serde_json::json;
        if !path.ends_with("/results") {
            return Ok(json!({"entry": [{"content": {"isDone": true, "dispatchState": "DONE"}}]}));
        }
        let q = |k: &str| query.iter().find(|(n, _)| *n == k).and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
        let (offset, count) = (q("offset"), q("count"));
        self.0.asked.lock().unwrap().push((offset, count));
        let results: Vec<Value> = (offset..(offset + count).min(self.0.rows)).map(|i| json!({"n": i.to_string(), "host": format!("h{}", i % 3), "_time": "x"})).collect();
        Ok(json!({"fields": [{"name": "n"}, {"name": "host"}, {"name": "_time"}], "results": results}))
    }
    fn post(&self, _: &str, _: &[(&str, &str)]) -> Result<Value, crate::ports::splunk::SplunkError> {
        Ok(serde_json::json!({"sid": "job1"}))
    }
    fn url(&self, path: &str) -> String {
        path.into()
    }
}

#[test]
fn splunk_results_load_into_a_table_in_chunks_and_page() {
    use crate::adapters::secondary::sqlite_store::SqliteStore;
    use crate::ports::{db::Database, service::Tables};
    use crate::usecases::{db::Db, splunk::Splunk};
    use serde_json::json;
    let dir = tmp("dbload");
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&dir));
    let fake = Arc::new(FakeSplunk { rows: 120_000, asked: std::sync::Mutex::new(Vec::new()) });
    let cfg = crate::ports::splunk::SplunkConfig { url: "https://splunk.test:8089".into(), token: "t".into(), ..Default::default() };
    let splunk = Splunk::new(Arc::new(LocalDisk), Arc::new(Arc::clone(&fake)), Arc::clone(&db), &dir, Some(cfg));
    let out = splunk.search_into("splunk-table", "search", "index=x", "-24h", "", &crate::ports::service::Unwatched).unwrap();
    assert_eq!(out["total"], 120_000);
    assert_eq!(out["columns"], json!(["n", "host"]), "internal fields stay out");
    assert_eq!(*fake.asked.lock().unwrap(), vec![(0, 50_000), (50_000, 50_000), (100_000, 50_000)], "read in chunks");

    let tables = Db::new(Arc::clone(&db));
    let page = tables.page("splunk-table", None, &json!({"table": "search", "offset": 0, "limit": 3, "orderBy": "n", "desc": true})).unwrap();
    assert_eq!(page["total"], 120_000);
    assert_eq!(page["rows"][0][0], json!(119_999), "n is numeric, so it sorts as a number");
    let filtered = tables.page("usl-lab", Some("splunk-table"), &json!({"table": "search", "where": "host = ?", "params": ["h1"], "limit": 2})).unwrap();
    assert_eq!(filtered["total"], 40_000, "another package reads the same table, read-only");
    assert!(tables.query("usl-lab", "SELECT count(*) FROM search", &json!([])).is_err(), "a package's own database does not hold another's tables");
    let _ = fs::remove_dir_all(dir);
}

// ---------- background jobs (ADR-2610072118) ----------

/// A Splunk whose search job runs until `done` is set, reads results slowly, and records a cancel.
struct SlowSplunk {
    rows: usize,
    done: std::sync::atomic::AtomicBool,
    cancelled: std::sync::atomic::AtomicBool,
}

struct SlowSession(Arc<SlowSplunk>);

impl crate::ports::splunk::SplunkApi for Arc<SlowSplunk> {
    fn connect(&self, _: &crate::ports::splunk::SplunkConfig) -> Result<Box<dyn crate::ports::splunk::SplunkSession>, String> {
        Ok(Box::new(SlowSession(Arc::clone(self))))
    }
}

impl crate::ports::splunk::SplunkSession for SlowSession {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, crate::ports::splunk::SplunkError> {
        use serde_json::json;
        use std::sync::atomic::Ordering;
        if !path.ends_with("/results") {
            let done = self.0.done.load(Ordering::SeqCst);
            return Ok(json!({"entry": [{"content": {"isDone": done, "dispatchState": if done { "DONE" } else { "RUNNING" }}}]}));
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
        let q = |k: &str| query.iter().find(|(n, _)| *n == k).and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
        let (offset, count) = (q("offset"), q("count"));
        let results: Vec<Value> = (offset..(offset + count).min(self.0.rows)).map(|i| json!({"n": i.to_string()})).collect();
        Ok(json!({"fields": [{"name": "n"}], "results": results}))
    }
    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<Value, crate::ports::splunk::SplunkError> {
        if path.ends_with("/job1/control") && form.contains(&("action", "cancel")) {
            self.0.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(serde_json::json!({"sid": "job1"}))
    }
    fn url(&self, path: &str) -> String {
        path.into()
    }
}

/// Claude is not used by these tests.
struct NoBuilder;

impl crate::ports::service::Builder for NoBuilder {
    fn status(&self) -> Value { Value::Null }
    fn set_key(&self, _: &str, _: Option<&str>) -> Result<Value, String> { Err("no".into()) }
    fn set_provider(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn send(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn sample(&self, _: &Value) -> Result<Value, String> { Err("no Claude here".into()) }
    fn stop(&self, _: &str) -> Result<Value, String> { Err("no".into()) }
    fn claim_test(&self, _: &str, _: usize) -> Result<Value, String> { Err("no".into()) }
    fn tested(&self, _: &str, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn sessions(&self) -> Value { Value::Null }
    fn events(&self, _: &str, _: usize) -> Result<Value, String> { Err("no".into()) }
}

fn job_runner(tag: &str, rows: usize) -> (crate::usecases::jobs::JobRunner, Arc<SlowSplunk>, std::path::PathBuf) {
    use crate::adapters::secondary::sqlite_store::SqliteStore;
    use crate::ports::db::Database;
    use crate::usecases::{jobs::JobRunner, splunk::Splunk};
    let dir = tmp(tag);
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&dir));
    let fake = Arc::new(SlowSplunk { rows, done: Default::default(), cancelled: Default::default() });
    let cfg = crate::ports::splunk::SplunkConfig { url: "https://splunk.test:8089".into(), token: "t".into(), ..Default::default() };
    let splunk = Splunk::new(Arc::new(LocalDisk), Arc::new(Arc::clone(&fake)), db, &dir, Some(cfg));
    (JobRunner::new(Arc::new(splunk), Arc::new(NoBuilder)), fake, dir)
}

/// Asks for the job every 50 ms until `until` holds, for at most 20 s.
fn poll_job(jobs: &dyn crate::ports::service::Jobs, id: &str, until: impl Fn(&Value) -> bool) -> Value {
    let t = std::time::Instant::now();
    loop {
        let j = jobs.get(id, Some("splunk-table")).unwrap();
        if until(&j) || t.elapsed() > std::time::Duration::from_secs(20) {
            return j;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn jobs_answer_at_once_report_progress_and_finish() {
    use crate::ports::service::{JobKind, Jobs};
    use serde_json::json;
    use std::sync::atomic::Ordering;
    let (jobs, fake, dir) = job_runner("jobs-run", 120_000);
    let t = std::time::Instant::now();
    let started = jobs.start(JobKind::SplunkInto, "splunk-table", "table", &json!({"search": "index=x", "table": "search"})).unwrap();
    assert!(t.elapsed() < std::time::Duration::from_millis(500), "start answers at once: {:?}", t.elapsed());
    let id = started["job"].as_str().unwrap().to_string();
    let j = jobs.get(&id, Some("splunk-table")).unwrap();
    assert_eq!(j["state"], "running");
    assert_eq!(j["kind"], "splunk.into");
    assert_eq!(j["label"], "index=x");
    std::thread::sleep(std::time::Duration::from_millis(700));
    assert_eq!(jobs.get(&id, Some("splunk-table")).unwrap()["state"], "running", "the Splunk job is still running");
    fake.done.store(true, Ordering::SeqCst);
    let mid = poll_job(&jobs, &id, |j| j["progress"].as_u64().is_some());
    assert_eq!(mid["state"], "running");
    assert_eq!(mid["progress"], 50_000, "rows loaded after the first chunk");
    let end = poll_job(&jobs, &id, |j| j["state"] != "running");
    assert_eq!(end["state"], "done");
    assert_eq!(end["progress"], 120_000);
    assert_eq!(end["result"]["total"], 120_000);
    assert!(jobs.list(Some("splunk-table"))["jobs"][0].get("result").is_none(), "lists leave results out");
    assert!(!fake.cancelled.load(Ordering::SeqCst));

    // Another package sees nothing of it; the admin sees everything.
    assert!(jobs.get(&id, Some("usl-lab")).is_err(), "another package's job is not found");
    assert!(jobs.cancel(&id, Some("usl-lab")).is_err(), "and cannot be cancelled by it");
    assert_eq!(jobs.list(Some("usl-lab"))["jobs"], json!([]));
    assert_eq!(jobs.list(Some("splunk-table"))["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(jobs.list(None)["jobs"].as_array().unwrap().len(), 1);
    assert!(jobs.get(&id, None).is_ok());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn jobs_cancel_stops_the_splunk_search_job() {
    use crate::ports::service::{JobKind, Jobs};
    use serde_json::json;
    use std::sync::atomic::Ordering;
    let (jobs, fake, dir) = job_runner("jobs-cancel", 10);
    let id = jobs.start(JobKind::SplunkSearch, "splunk-table", "table", &json!({"search": "index=slow"})).unwrap()["job"].as_str().unwrap().to_string();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let c = jobs.cancel(&id, Some("splunk-table")).unwrap();
    assert_eq!(c["state"], "cancelled");
    let t = std::time::Instant::now();
    while !fake.cancelled.load(Ordering::SeqCst) && t.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(fake.cancelled.load(Ordering::SeqCst), "the search job on Splunk was cancelled too");
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(jobs.get(&id, Some("splunk-table")).unwrap()["state"], "cancelled", "and the job stays cancelled");
    assert!(jobs.cancel(&id, Some("splunk-table")).is_ok(), "cancelling again does no harm");
    let _ = fs::remove_dir_all(dir);
}

// ---------- export as a .wardian file (ADR-2610071248) ----------

#[test]
fn export_with_data_round_trips_and_never_carries_secrets() {
    use crate::adapters::secondary::sqlite_store::SqliteStore;
    use crate::ports::{db::Database, service::{Exports, ViewerState}};
    use crate::usecases::{export::Exporter, history::History, import::import_zip, viewer_state::State, workspace::copy_tree};
    use serde_json::json;
    use std::io::Read;

    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let exporter = |data: &Path| -> (Exporter, Arc<dyn ViewerState>, Arc<dyn Database>) {
        let apps = data.join("apps");
        let state: Arc<dyn ViewerState> = Arc::new(State::new(Arc::clone(&fs), data));
        let db: Arc<dyn Database> = Arc::new(SqliteStore::new(data));
        let history = Arc::new(History::new(Arc::clone(&fs), data, &apps));
        (Exporter::new(Arc::clone(&fs), Arc::new(checker()), Arc::clone(&db), Arc::clone(&state), history, &apps, data), state, db)
    };

    // A Wardian with the loan planner, its data, a build folder, history, and every secret filled.
    let a = tmp("export-a");
    copy_tree(&*fs, Path::new("apps/loan-planner"), &a.join("apps/loan-planner")).unwrap();
    fs::create_dir_all(a.join("apps/loan-planner/target/debug")).unwrap();
    fs::write(a.join("apps/loan-planner/target/debug/x"), b"build output").unwrap();
    fs::write(a.join("anthropic-key"), b"sk-ant-SECRET-KEY").unwrap();
    fs::write(a.join("anthropic-workspace"), b"wrkspc_SECRET").unwrap();
    fs::write(a.join("bedrock.json"), br#"{"region":"us-east-1","token":"BEDROCK-SECRET"}"#).unwrap();
    fs::write(a.join("splunk.json"), br#"{"url":"https://s:8089","password":"SPLUNK-SECRET"}"#).unwrap();
    fs::write(a.join("grants.json"), br#"[{"app":"loan-planner","channel":"GRANT-SECRET","mode":"use","allow":true}]"#).unwrap();
    let (ex_a, state_a, db_a) = exporter(&a);
    state_a.set_app_value("loan-planner", "inputs", "state", json!({"principal": 320000})).unwrap();
    state_a.set_layout("loan-planner", json!({"v": 1, "hidden": ["export"]})).unwrap();
    db_a.create_table("loan-planner", "payments", &["month".into(), "amount".into()], &["INTEGER", "NUMERIC"], true).unwrap();
    db_a.insert_rows("loan-planner", "payments", &["month".into(), "amount".into()], &[vec![json!(1), json!(1798.65)], vec![json!(2), json!(1798.65)]]).unwrap();

    // App only, then with data.
    let plain = ex_a.export("loan-planner", false).unwrap();
    let full = ex_a.export("loan-planner", true).unwrap();
    let unpack = |bytes: &[u8]| -> Vec<(String, Vec<u8>)> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut b = Vec::new(); f.read_to_end(&mut b).unwrap(); (f.name().to_string(), b) }).collect()
    };
    let plain_files = unpack(&plain);
    let full_files = unpack(&full);
    assert!(plain_files.iter().all(|(n, _)| n.starts_with("loan-planner/")), "one package folder at the top");
    assert!(plain_files.iter().all(|(n, _)| !n.contains("/target/") && !n.contains("/.git/") && !n.contains("history")), "no build output or history");
    assert!(!plain_files.iter().any(|(n, _)| n.contains("/.wardian/data/")), "no data unless asked");
    for f in ["storage.json", "layout.json", "tables.sqlite"] {
        assert!(full_files.iter().any(|(n, _)| n == &format!("loan-planner/.wardian/data/{f}")), "{f} is in the file with data");
    }
    for (name, body) in plain_files.iter().chain(full_files.iter()) {
        let text = String::from_utf8_lossy(body);
        for secret in ["SECRET-KEY", "wrkspc_SECRET", "BEDROCK-SECRET", "SPLUNK-SECRET", "GRANT-SECRET"] {
            assert!(!text.contains(secret), "{name} carries {secret}");
        }
    }

    // The file passes `wardian check`, as any host imports it.
    let file = a.join("loan-planner.wardian");
    fs::write(&file, &full).unwrap();
    assert!(passes(&file), "an exported file passes wardian check");

    // Preview before import: the manifest, what the app may use, its data.
    let b = tmp("export-b");
    let (ex_b, state_b, db_b) = exporter(&b);
    let preview = ex_b.preview_import(&full).unwrap();
    assert_eq!(preview["package"], "loan-planner");
    assert_eq!(preview["data"]["tables"][0]["rows"], 2);

    // Into a fresh Wardian, without the data: the app only.
    import_zip(&*fs, &full, "loan-planner.wardian", &b.join("apps"), false, &|_| {}).unwrap();
    assert!(b.join("apps/loan-planner/suite.json").is_file());
    assert!(!b.join("apps/loan-planner/.wardian").exists(), "the manifest folder is not installed as part of the app");
    assert_eq!(state_b.app_data("loan-planner").unwrap(), json!({}), "no data unless asked");

    // Then with it: the same data, layout and tables; no permission answers come along.
    ex_b.install_data("loan-planner", &full).unwrap();
    assert_eq!(state_b.app_data("loan-planner").unwrap()["inputs"]["state"]["principal"], 320000);
    assert_eq!(state_b.layout("loan-planner").unwrap()["hidden"][0], "export");
    assert_eq!(db_b.tables("loan-planner").unwrap()[0]["rows"], 2);
    assert!(!b.join("grants.json").exists() && !b.join("anthropic-key").exists());

    // Replacing installed data keeps what was there next to the app's latest version.
    state_b.set_app_value("loan-planner", "inputs", "state", json!({"principal": 1})).unwrap();
    ex_b.keep_data_before_import("loan-planner").unwrap();
    let kept = b.join("history/loan-planner/1.data/storage.json");
    assert!(String::from_utf8_lossy(&fs::read(&kept).unwrap()).contains("\"principal\": 1"), "the replaced data is kept in history");

    // A package that fails the check is not exported.
    fs::write(b.join("apps/loan-planner/suite.json"), b"{ not json").unwrap();
    assert!(ex_b.export("loan-planner", false).is_err());
    let _ = fs::remove_dir_all(a);
    let _ = fs::remove_dir_all(b);
}

// ---------- hostile imports (ADR-2610072033) ----------

/// A zip of these (name, bytes) entries, compressed as an exporter would.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        z.start_file(*name, opts).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// A zip with one symbolic link among its files.
fn zip_with_link(files: &[(&str, &[u8])], link: &str, target: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        z.start_file(*name, opts).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.add_symlink(link, target, opts).unwrap();
    z.finish().unwrap().into_inner()
}

/// Each central directory record of a zip: (where it is, where its local header is, its name).
fn zip_records(zip: &[u8]) -> Vec<(usize, usize, String)> {
    let end = (0..=zip.len() - 22).rev().find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).unwrap();
    let le16 = |i: usize| usize::from(u16::from_le_bytes([zip[i], zip[i + 1]]));
    let le32 = |i: usize| u32::from_le_bytes(zip[i..i + 4].try_into().unwrap()) as usize;
    let mut at = le32(end + 16);
    let mut out = Vec::new();
    for _ in 0..le16(end + 10) {
        let (n, e, c) = (le16(at + 28), le16(at + 30), le16(at + 32));
        out.push((at, le32(at + 42), String::from_utf8_lossy(&zip[at + 46..at + 46 + n]).into_owned()));
        at += 46 + n + e + c;
    }
    out
}

/// Makes a zip lie: entry `name` says, in both its headers, that it unpacks to `size` bytes.
fn claim_size(zip: &mut [u8], name: &str, size: u32) {
    for (cd, local, n) in zip_records(zip) {
        if n == name {
            zip[cd + 24..cd + 28].copy_from_slice(&size.to_le_bytes());
            zip[local + 22..local + 26].copy_from_slice(&size.to_le_bytes());
        }
    }
}

/// Renames an entry in both its headers to a name of the same length, as a hand-made zip could.
fn rename_entry(zip: &mut [u8], from: &str, to: &str) {
    assert_eq!(from.len(), to.len());
    for (cd, local, n) in zip_records(zip) {
        if n == from {
            zip[cd + 46..cd + 46 + to.len()].copy_from_slice(to.as_bytes());
            zip[local + 30..local + 30 + to.len()].copy_from_slice(to.as_bytes());
        }
    }
}

/// Every file under `dir`, hidden ones included, so a test can see that nothing was written.
fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files_under(&p));
        } else {
            out.push(p.display().to_string());
        }
    }
    out
}

#[test]
fn hostile_zips_are_refused_and_nothing_lands_outside() {
    use crate::usecases::import::import_zip;
    let disk = LocalDisk;
    let base = tmp("hostile");
    let apps = base.join("inside/apps");
    fs::create_dir_all(&apps).unwrap();
    let outside = std::env::temp_dir().join(format!("wardian-escaped-{}", std::process::id()));
    let _ = fs::remove_dir_all(&outside);
    let page: &[u8] = b"<p>hi</p>";
    let app = |extra: &[(&str, &[u8])]| {
        let mut v: Vec<(&str, &[u8])> = vec![("hello/app.wasm", EMPTY_WASM), ("hello/index.html", page)];
        v.extend_from_slice(extra);
        zip_of(&v)
    };
    let refused = |bytes: &[u8], why: &str| {
        let e = import_zip(&disk, bytes, "x.zip", &apps, true, &|_| {}).err().unwrap_or_else(|| panic!("{why}: the zip was imported"));
        assert!(e.contains(why) && e.contains("refused"), "expected \"{why}\", got: {e}");
        assert_eq!(files_under(&base), Vec::<String>::new(), "{why}: nothing may be written");
    };

    // Paths that leave the folder, absolute paths and drives.
    let absolute = format!("{}/app.wasm", outside.display());
    refused(&app(&[("../escaped/app.wasm", EMPTY_WASM)]), "leaves its folder");
    refused(&app(&[("hello/../../../escaped.txt", b"x")]), "leaves its folder");
    refused(&app(&[("..\\..\\escaped.txt", b"x")]), "leaves its folder");
    refused(&app(&[(absolute.as_str(), EMPTY_WASM)]), "absolute path");
    refused(&app(&[("C:\\escaped.txt", b"x")]), "names a drive");

    // Links: refused where they would be installed, left out where nothing is installed.
    let files: [(&str, &[u8]); 2] = [("hello/app.wasm", EMPTY_WASM), ("hello/index.html", page)];
    refused(&zip_with_link(&files, "hello/passwd", "/etc/passwd"), "symbolic link");
    refused(&zip_with_link(&files, "hello/ui", "../../.."), "symbolic link");
    let ignored = zip_with_link(&files, "hello/node_modules/.bin/tool", "/bin/sh");
    let done = import_zip(&disk, &ignored, "x.zip", &apps, true, &|_| {}).unwrap();
    assert!(done.skipped.iter().any(|s| s.contains("a link")), "{:?}", done.skipped);
    assert!(!apps.join("hello/node_modules").exists());
    fs::remove_dir_all(apps.join("hello")).unwrap();

    // Zip bombs: a file that says it is huge, files that say they add up past the limit, and a
    // file that says it is small but unpacks to more than any file may.
    let mut huge = app(&[("hello/big.bin", b"small")]);
    claim_size(&mut huge, "hello/big.bin", 3_000_000_000);
    refused(&huge, "at most 64 MB");
    let parts: Vec<(String, Vec<u8>)> = (0..5).map(|i| (format!("hello/part{i}.bin"), vec![b'x'])).collect();
    let mut many = app(&parts.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect::<Vec<_>>());
    for (n, _) in &parts {
        claim_size(&mut many, n, 60 * 1024 * 1024);
    }
    refused(&many, "more than 256 MB");
    let zeros = vec![0u8; 65 * 1024 * 1024];
    let mut liar = app(&[("hello/big.bin", &zeros)]);
    assert!(liar.len() < 1024 * 1024, "a bomb: {} bytes that unpack to 65 MB", liar.len());
    claim_size(&mut liar, "hello/big.bin", 1000);
    refused(&liar, "more than 64 MB");

    // The same package twice: by folder, by case, and by naming one file twice.
    refused(&zip_of(&[("a/hello/app.wasm", EMPTY_WASM), ("b/hello/app.wasm", EMPTY_WASM)]), "two apps named");
    refused(&zip_of(&[("hello/app.wasm", EMPTY_WASM), ("Hello/app.wasm", EMPTY_WASM)]), "two apps named");
    let mut twice = zip_of(&[("hello/app.wasm", EMPTY_WASM), ("hello/index.html", b"<p>shown</p>"), ("hello/index.htmX", b"<p>hidden</p>")]);
    rename_entry(&mut twice, "hello/index.htmX", "hello/index.html");
    refused(&twice, "more than once");

    assert!(!outside.exists(), "nothing was written outside the apps folder");
    // A sound zip still imports.
    import_zip(&disk, &app(&[]), "x.zip", &apps, false, &|_| {}).unwrap();
    assert!(apps.join("hello/app.wasm").is_file());
    let _ = fs::remove_dir_all(base);
}

#[test]
fn hostile_data_in_a_wardian_file_is_refused_and_the_app_keeps_its_own() {
    use crate::adapters::secondary::sqlite_store::SqliteStore;
    use crate::ports::{db::Database, service::{Exports, ViewerState}};
    use crate::usecases::{export::Exporter, history::History, import::import_zip, viewer_state::State};
    use serde_json::json;

    let disk: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let data = tmp("hostile-data");
    let apps = data.join("apps");
    let state: Arc<dyn ViewerState> = Arc::new(State::new(Arc::clone(&disk), &data));
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&data));
    let history = Arc::new(History::new(Arc::clone(&disk), &data, &apps));
    let ex = Exporter::new(Arc::clone(&disk), Arc::new(checker()), Arc::clone(&db), Arc::clone(&state), history, &apps, &data);

    // The app is installed and has data of its own.
    import_zip(&*disk, &zip_of(&[("hello/app.wasm", EMPTY_WASM)]), "hello.zip", &apps, false, &|_| {}).unwrap();
    db.create_table("hello", "kept", &["a".into()], &["INTEGER"], true).unwrap();
    db.insert_rows("hello", "kept", &["a".into()], &[vec![json!(7)]]).unwrap();
    state.set_app_value("hello", "main", "k", json!("mine")).unwrap();

    // A real database to carry.
    let carried = data.join("carried.sqlite");
    let c = rusqlite::Connection::open(&carried).unwrap();
    c.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);").unwrap();
    drop(c);
    let real = fs::read(&carried).unwrap();
    let manifest = |package: &str| json!({"format": 1, "package": package, "includes": {"app": true, "data": true}}).to_string();
    let storage = json!({"main": {"k": "theirs"}}).to_string();
    let file = |folder: &str, package: &str, tables: &[u8]| {
        let m = manifest(package);
        let names = [format!("{folder}/app.wasm"), format!("{folder}/.wardian/export.json"), format!("{folder}/.wardian/data/storage.json"), format!("{folder}/.wardian/data/tables.sqlite")];
        zip_of(&[(&names[0], EMPTY_WASM), (&names[1], m.as_bytes()), (&names[2], storage.as_bytes()), (&names[3], tables)])
    };
    let unchanged = |why: &str| {
        assert_eq!(db.query("hello", "SELECT a FROM kept", &[]).unwrap()["rows"][0][0], 7, "{why}: the app keeps its tables");
        assert_eq!(state.app_data("hello").unwrap()["main"]["k"], "mine", "{why}: the app keeps its storage");
    };
    let refused = |bytes: &[u8], why: &str, at_preview: bool| {
        if at_preview {
            let e = ex.preview_import(bytes).err().unwrap_or_else(|| panic!("{why}: the preview accepted it"));
            assert!(e.contains(why), "preview: expected \"{why}\", got: {e}");
        }
        let e = ex.install_data("hello", bytes).err().unwrap_or_else(|| panic!("{why}: the data was installed"));
        assert!(e.contains(why), "install: expected \"{why}\", got: {e}");
        unchanged(why);
    };

    let mut oversize = file("hello", "hello", &real);
    claim_size(&mut oversize, "hello/.wardian/data/tables.sqlite", 300 * 1024 * 1024);
    refused(&oversize, "data files may be at most 256 MB", true);
    refused(&file("hello", "hello", b"<html>not a database</html>"), "not an SQLite database", true);
    let mut damaged = b"SQLite format 3\0".to_vec();
    damaged.extend(std::iter::repeat_n(0xAB, 8192));
    refused(&file("hello", "hello", &damaged), "tables were not installed", false);
    refused(&file("hello", "other", &real), "names the app \"other\"", true);
    let m = manifest("hello");
    let two = zip_of(&[("hello/app.wasm", EMPTY_WASM), ("hello/.wardian/export.json", m.as_bytes()), ("hello2/app.wasm", EMPTY_WASM), ("hello2/.wardian/export.json", m.as_bytes())]);
    refused(&two, "more than one exported app", true);
    let linked = zip_with_link(&[("hello/app.wasm", EMPTY_WASM), ("hello/.wardian/export.json", m.as_bytes())], "hello/.wardian/data/tables.sqlite", "/etc/passwd");
    refused(&linked, "not a plain file", true);

    // The sound file installs.
    let good = file("hello", "hello", &real);
    ex.preview_import(&good).unwrap();
    ex.install_data("hello", &good).unwrap();
    assert_eq!(db.query("hello", "SELECT x FROM t", &[]).unwrap()["rows"][0][0], 1);
    assert_eq!(state.app_data("hello").unwrap()["main"]["k"], "theirs");
    let _ = fs::remove_dir_all(data);
}

// ---------- secrets never leave (ADR-2610072033) ----------

/// A stand-in for Anthropic, Bedrock, Splunk and Google: every request gets 200 and one JSON body
/// holding the fields each of their clients reads. Returns its address.
fn fake_upstream() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    const REPLY: &str = r#"{"id":"msg_1","type":"message","role":"assistant","model":"m","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1},"entry":[{"content":{"username":"admin","serverName":"fake","version":"9.0"}}]}"#;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim_end().to_ascii_lowercase();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let _ = write!(&conn, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{REPLY}", REPLY.len());
            });
        }
    });
    format!("http://{addr}")
}

/// Not a test by itself: `TestServer` runs this binary again with WARDIAN_TEST_SERVER set, and
/// this starts the real server, so a test can talk to it and read everything it prints.
#[test]
fn server_child() {
    let Ok(apps) = std::env::var("WARDIAN_TEST_SERVER") else { return };
    crate::serve(crate::config::Settings::from_env(Some(&apps), &LocalDisk), true);
}

/// The real server in a child process, serving `apps` with `data` as its data folder, and every
/// line it prints. The child gets only `env`, never this process's keys or ADMIN_TOKEN.
struct TestServer {
    addr: String,
    child: std::process::Child,
    log: Arc<std::sync::Mutex<String>>,
    readers: Vec<std::thread::JoinHandle<()>>,
}

impl TestServer {
    /// Starts the child; `addr` is the address it was told to listen on.
    fn spawn(apps: &Path, data: &Path, addr: &str, env: &[(&str, &str)]) -> (TestServer, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["tests::server_child", "--exact", "--nocapture", "--test-threads=1"]).env_clear();
        for keep in ["PATH", "HOME", "TMPDIR"] {
            if let Some(v) = std::env::var_os(keep) {
                cmd.env(keep, v);
            }
        }
        let mut child = cmd
            .env("WARDIAN_TEST_SERVER", apps)
            .env("DATA_DIR", data)
            .env("ADDR", addr)
            .envs(env.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let log = Arc::new(std::sync::Mutex::new(String::new()));
        let (port_tx, port_rx) = std::sync::mpsc::channel();
        let mut readers = Vec::new();
        for (stream, port_tx) in [(Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>, Some(port_tx)), (Box::new(child.stderr.take().unwrap()), None)] {
            let log = Arc::clone(&log);
            readers.push(std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if let (Some(tx), Some(addr)) = (&port_tx, line.split("listening on http://").nth(1)) {
                        let _ = tx.send(addr.trim().to_string());
                    }
                    log.lock().unwrap().push_str(&format!("{line}\n"));
                }
            }));
        }
        (TestServer { addr: addr.to_string(), child, log, readers }, port_rx)
    }

    /// Starts the child on a free port and waits until it listens.
    fn start(apps: &Path, data: &Path, env: &[(&str, &str)]) -> TestServer {
        let (mut server, port_rx) = TestServer::spawn(apps, data, "127.0.0.1:0", env);
        match port_rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(addr) => server.addr = addr,
            Err(_) => panic!("the server did not start:\n{}", server.stop()),
        }
        server
    }

    /// Waits for the child to end by itself, for at most 30 s, and returns everything it printed.
    fn wait(mut self) -> String {
        let t = std::time::Instant::now();
        while self.child.try_wait().unwrap().is_none() && t.elapsed() < std::time::Duration::from_secs(30) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.stop()
    }

    /// Stops the child and returns everything it printed.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for r in self.readers.drain(..) {
            let _ = r.join();
        }
        let log = self.log.lock().unwrap().clone();
        log
    }

    /// One request on a new connection, sent as written: with `Host: localhost` unless `headers`
    /// names another, and no `Connection` header. Returns the status, the response head and the body.
    fn send(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, String, Vec<u8>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let mut conn = std::net::TcpStream::connect(&self.addr).unwrap();
        conn.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();
        let mut head = format!("{method} {path} HTTP/1.1\r\n");
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("Host")) {
            head.push_str("Host: localhost\r\n");
        }
        for (k, v) in headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        conn.write_all(head.as_bytes()).unwrap();
        conn.write_all(body).unwrap();
        let mut reader = BufReader::new(conn);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            head.push_str(&line);
        }
        let status = head.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
        let len = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
        let mut body = vec![0; len];
        reader.read_exact(&mut body).unwrap();
        (status, head, body)
    }

    /// A JSON request from this machine (Host: localhost), with `headers` added. Returns the status
    /// and the answer.
    fn json(&self, method: &str, path: &str, body: Option<Value>, headers: &[(&str, &str)]) -> (u16, Value) {
        let mut h = headers.to_vec();
        let bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        if !bytes.is_empty() {
            h.push(("Content-Type", "application/json"));
        }
        let (status, _, answer) = self.send(method, path, &h, &bytes);
        (status, serde_json::from_slice(&answer).unwrap_or(Value::Null))
    }
}

/// A suite folder named `name` in `apps`, whose apps (name, caps) declare only what they are given.
fn tiny_suite(apps: &Path, name: &str, members: &[(&str, &[&str])]) {
    let list: Vec<Value> = members.iter().map(|(n, caps)| serde_json::json!({"name": n, "caps": caps})).collect();
    fs::create_dir_all(apps.join(name)).unwrap();
    fs::write(apps.join(name).join("suite.json"), serde_json::json!({"format": 1, "title": name, "apps": list}).to_string()).unwrap();
}

/// Every key and account filled, from the environment and through Settings, then every answer
/// that shows settings or status, an export with data and its import preview, and every line the
/// server printed, are searched for them.
#[test]
fn secrets_never_leave_in_answers_exports_or_logs() {
    use crate::usecases::workspace::copy_tree;
    use serde_json::json;
    use std::io::Read;
    use std::sync::Mutex;

    let up = fake_upstream();
    let base = tmp("secrets");
    let (data, apps) = (base.join("data"), base.join("apps"));
    copy_tree(&LocalDisk, Path::new("apps/loan-planner"), &apps.join("loan-planner")).unwrap();
    let key_file = |marker: &str| json!({"type": "service_account", "client_email": "wardian@example.iam.gserviceaccount.com", "token_uri": format!("{up}/token"),
        "private_key": format!("-----BEGIN PRIVATE KEY-----\n{marker}\n-----END PRIVATE KEY-----\n")});
    fs::write(base.join("sa.json"), key_file("ENV-GOOGLE-PRIVATE-KEY-SECRET").to_string()).unwrap();
    let admin = "ADMIN-TOKEN-SECRET-0123456789";
    let env = [
        ("ANTHROPIC_API_KEY", "sk-ant-ENV-ANTHROPIC-SECRET"),
        ("AWS_BEARER_TOKEN_BEDROCK", "ENV-BEDROCK-API-KEY-SECRET"),
        ("AWS_ACCESS_KEY_ID", "AKIAENVACCESSKEYSECRET"),
        ("AWS_SECRET_ACCESS_KEY", "ENV-AWS-SECRET-ACCESS-KEY-SECRET"),
        ("AWS_SESSION_TOKEN", "ENV-AWS-SESSION-TOKEN-SECRET"),
        ("SPLUNK_PASSWORD", "ENV-SPLUNK-PASSWORD-SECRET"),
    ];
    let typed = [
        "sk-ant-SET-ANTHROPIC-SECRET",
        "SET-BEDROCK-API-KEY-SECRET",
        "AKIASETACCESSKEYSECRET",
        "SET-AWS-SECRET-ACCESS-KEY-SECRET",
        "SET-AWS-SESSION-TOKEN-SECRET",
        "SET-SPLUNK-TOKEN-SECRET",
        "SET-SPLUNK-PASSWORD-SECRET",
        "POSTED-GOOGLE-PRIVATE-KEY-SECRET",
    ];

    let sa = base.join("sa.json").display().to_string();
    let mut all_env = env.to_vec();
    all_env.extend([
        ("ADMIN_TOKEN", admin),
        ("ANTHROPIC_WORKSPACE_ID", "wrkspc_ENVWORKSPACESECRET"),
        ("ANTHROPIC_BASE_URL", up.as_str()),
        ("AWS_REGION", "us-east-1"),
        ("WARDIAN_BEDROCK_BASE_URL", up.as_str()),
        ("WARDIAN_AI_PROVIDER", "bedrock"),
        ("SPLUNK_URL", up.as_str()),
        ("SPLUNK_USERNAME", "admin"),
        ("GDRIVE_SA_KEY", sa.as_str()),
        ("GDRIVE_API_BASE", up.as_str()),
    ]);
    let server = TestServer::start(&apps, &data, &all_env);
    let addr = server.addr.clone();

    // Every answer the server gives, with what was asked.
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(30)).build();
    // (what was asked, whether by an admin, the answer)
    let answers: Mutex<Vec<(String, bool, Vec<u8>)>> = Mutex::new(Vec::new());
    let ask = |method: &str, path: &str, body: Option<(&str, Vec<u8>)>, token: bool| -> Vec<u8> {
        let mut req = agent.request(method, &format!("http://{addr}{path}"));
        if token {
            req = req.set("X-Admin-Token", admin);
        }
        let resp = match body {
            Some((ct, b)) => req.set("Content-Type", ct).send_bytes(&b),
            None => req.call(),
        };
        let resp = match resp {
            Ok(r) | Err(ureq::Error::Status(_, r)) => r,
            Err(e) => panic!("{method} {path}: {e}"),
        };
        let mut bytes = Vec::new();
        resp.into_reader().read_to_end(&mut bytes).unwrap();
        answers.lock().unwrap().push((format!("{method} {path}"), token, bytes.clone()));
        bytes
    };
    let post = |path: &str, v: serde_json::Value| -> serde_json::Value {
        serde_json::from_slice(&ask("POST", path, Some(("application/json", v.to_string().into_bytes())), true)).unwrap_or_default()
    };

    // Fill every setting through Settings, as a user would; each is tested against the stand-in.
    let ai = post("/api/ai/key", json!({"key": typed[0], "workspace": "wrkspc_SETWORKSPACESECRET"}));
    assert_eq!(ai["anthropic"]["ready"], true, "{ai}");
    let bedrock = post("/api/ai/provider", json!({"provider": "bedrock", "region": "us-east-1", "auth": "api-key", "token": typed[1]}));
    assert_eq!(bedrock["bedrock"]["settings"]["from"], "settings", "{bedrock}");
    let bedrock = post("/api/ai/provider", json!({"provider": "bedrock", "region": "us-west-2", "auth": "access-keys", "access_key_id": typed[2], "secret_access_key": typed[3], "session_token": typed[4]}));
    assert_eq!(bedrock["bedrock"]["settings"]["auth"], "access-keys", "{bedrock}");
    let splunk = post("/api/splunk/config", json!({"url": up, "token": typed[5]}));
    assert_eq!(splunk["from"], "settings", "{splunk}");
    let splunk = post("/api/splunk/config", json!({"url": up, "username": "admin", "password": typed[6]}));
    assert_eq!(splunk["auth"], "user admin", "{splunk}");
    post("/api/drive/key", key_file(typed[7]));
    // Settings that are refused must not echo what was typed either.
    post("/api/ai/key", json!({"key": format!("{} bad", typed[0])}));
    post("/api/ai/provider", json!({"provider": "bedrock", "region": "Not A Region", "auth": "api-key", "token": typed[1]}));
    post("/api/splunk/config", json!({"url": "ftp://nowhere", "password": typed[6]}));
    post("/api/drive/key", json!({"private_key": [typed[7]]}));
    post("/api/state/apps/loan-planner", json!({"app": "inputs", "key": "state", "value": {"principal": 320000}}));

    // Everything that shows settings or status, with and without the token.
    for path in ["/api/status", "/api/grants", "/api/apps", "/api/app-list", "/api/trash", "/api/ai/sessions", "/api/history/loan-planner", "/api/state/apps/loan-planner", "/api/state/layout/loan-planner", "/api/apps/loan-planner/export?data=1&preview=1"] {
        ask("GET", path, None, true);
    }
    for path in ["/api/status", "/api/grants", "/api/apps", "/api/app-list", "/api/ai/sessions", "/api/drive/browse"] {
        ask("GET", path, None, false);
    }
    let status: serde_json::Value = serde_json::from_slice(&ask("GET", "/api/status", None, true)).unwrap();
    assert_eq!((status["ai"]["provider"].as_str(), status["splunk"]["ready"].as_bool()), (Some("bedrock"), Some(true)), "{status}");

    // An export with data, and what an import shows of it.
    let export = ask("GET", "/api/apps/loan-planner/export?data=1", None, true);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(export.clone())).expect("an export");
    assert!(zip.file_names().any(|n| n == "loan-planner/.wardian/data/storage.json"), "the export carries data");
    let mut inside = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        inside.push((format!("export: {}", f.name()), false, b));
    }
    ask("POST", "/api/import/preview", Some(("application/zip", export)), true);

    let log = server.stop();
    assert!(log.contains("admin: whoever sends ADMIN_TOKEN") && log.contains("export: loan-planner with its data"), "the server's log was read:\n{log}");

    let mut searched = answers.into_inner().unwrap();
    searched.extend(inside);
    searched.push(("the server's log".into(), false, log.into_bytes()));
    let secrets: Vec<&str> = env.iter().map(|(_, v)| *v).chain(typed).chain(["ENV-GOOGLE-PRIVATE-KEY-SECRET", admin]).collect();
    // The workspace ID is an identifier, not a key: Settings shows it to an admin, and nobody else.
    let workspaces = ["wrkspc_SETWORKSPACESECRET", "wrkspc_ENVWORKSPACESECRET"];
    for (what, by_admin, body) in &searched {
        let text = String::from_utf8_lossy(body);
        for s in secrets.iter().chain(if *by_admin { &[][..] } else { &workspaces[..] }) {
            assert!(!text.contains(s), "{what} carries the secret {s}:\n{text}");
        }
    }
    assert!(searched.len() > 30, "{} answers searched", searched.len());
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610080930: with ADDR unset, a port held by a plain listener (not a Wardian) moves Wardian
/// to the next free port, and the usual port is named as busy.
#[test]
fn start_takes_the_next_port_when_a_plain_listener_holds_it() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    match crate::take_address(&format!("127.0.0.1:{port}"), false, port.saturating_add(5)) {
        crate::Address::Ready(listener, busy) => {
            let got = listener.local_addr().unwrap().port();
            assert!(got > port && got <= port.saturating_add(5), "took {got}, held {port}");
            assert_eq!(busy, Some(port));
        }
        crate::Address::Running(at) => panic!("a plain listener is not a Wardian: {at}"),
        crate::Address::Failed(what, todo) => panic!("{what} {todo}"),
    }
    // With ADDR set, the same busy port is an error that says what to do.
    match crate::take_address(&format!("127.0.0.1:{port}"), true, port.saturating_add(5)) {
        crate::Address::Failed(what, todo) => {
            assert!(what.contains(&format!("cannot listen on 127.0.0.1:{port}")), "{what}");
            assert!(todo.contains("ADDR="), "{todo}");
        }
        _ => panic!("ADDR set: a busy port must be an error"),
    }
    drop(held);
}

/// ADR-2610080930: with ADDR unset, a Wardian answering /api/status on the port is reported as
/// running, and nothing is taken.
#[test]
fn start_finds_a_wardian_already_running() {
    use std::io::{Read, Write};
    let fake = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = fake.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in fake.incoming().flatten().take(4) {
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let body = r#"{"source":"local","local_root":"apps","apps":5,"first_run":false,"admin":true}"#;
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    match crate::take_address(&format!("127.0.0.1:{port}"), false, port.saturating_add(5)) {
        crate::Address::Running(at) => assert_eq!(at, format!("127.0.0.1:{port}")),
        crate::Address::Ready(l, _) => panic!("took {:?} although a Wardian runs on {port}", l.local_addr()),
        crate::Address::Failed(what, todo) => panic!("{what} {todo}"),
    }
}

// ---------- every claim names its test (ADR-2610081041) ----------

/// The catalog over `apps`, as the server builds it, with the real disk.
fn hub(apps: &Path, data: &Path) -> crate::usecases::catalog::Hub {
    use crate::adapters::secondary::google_drive::GoogleDrive;
    hub_with_drive(apps, data, Arc::new(GoogleDrive::new("https://drive.invalid")))
}

fn hub_with_drive(apps: &Path, data: &Path, drive: Arc<dyn crate::ports::drive::DriveConnector>) -> crate::usecases::catalog::Hub {
    use crate::adapters::secondary::link_fetch::LinkFetcher;
    use crate::usecases::{catalog::{Hub, HubPorts}, history::History};
    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let ports = HubPorts { fs: Arc::clone(&fs), drive, web: Arc::new(LinkFetcher::new(false)), assets: Arc::new(Embedded) };
    let history = Arc::new(History::new(fs, data, apps));
    Hub::new(ports, history, data.to_path_buf(), apps.to_path_buf(), std::time::Duration::from_secs(60))
}

/// A Google that accepts any service account key, and has nothing in Drive.
struct AnyDriveKey;

impl crate::ports::drive::DriveConnector for AnyDriveKey {
    fn client(&self, _: &str) -> Result<Arc<dyn crate::ports::drive::DriveClient>, String> {
        Ok(Arc::new(AnyDriveKey))
    }
}

impl crate::ports::drive::DriveClient for AnyDriveKey {
    fn client_email(&self) -> String { "w@example.iam.gserviceaccount.com".into() }
    fn check(&self) -> Result<(), String> { Ok(()) }
    fn browse(&self, _: Option<&str>) -> Result<Vec<crate::domain::package::Folder>, String> { Ok(Vec::new()) }
    fn preview(&self, _: &str) -> Result<Vec<String>, String> { Ok(Vec::new()) }
    fn download(&self, _: &str) -> Result<Vec<u8>, String> { Err("nothing here".into()) }
    fn open_folder(self: Arc<Self>, _: &str, _: &str, _: std::time::Duration, _: bool) -> Result<Arc<dyn crate::ports::drive::DriveFolder>, String> {
        Err("nothing here".into())
    }
}

/// security.md Admin (#135, #138, #140): without ADMIN_TOKEN, a request whose Host is not loopback
/// is not an admin (DNS rebinding), and a change needs a JSON or zip Content-Type (no form posts
/// from other sites). Neither changes anything.
#[test]
fn claim_admin_gate_without_a_token() {
    use serde_json::json;
    let base = tmp("claim-admin");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "s", &[("a", &["splunk"])]);
    let server = TestServer::start(&apps, &data, &[]);
    let grant = json!({"app": "s", "channel": "splunk", "mode": "use", "decision": "allow"}).to_string();
    let json_ct = ("Content-Type", "application/json");
    for host in ["evil.com", "evil.com:8000", "localhost.evil.com", "127.0.0.1.evil.com", "[::1].evil.com"] {
        let (status, _, body) = server.send("POST", "/api/grants", &[("Host", host), json_ct], grant.as_bytes());
        assert_eq!(status, 403, "Host: {host} {}", String::from_utf8_lossy(&body));
        let (status, _, _) = server.send("POST", "/api/import", &[("Host", host), ("Content-Type", "application/zip")], b"PK");
        assert_eq!(status, 403, "import with Host: {host}");
        assert_ne!(server.send("GET", "/api/trash", &[("Host", host)], b"").0, 200, "GET /api/trash with Host: {host}");
    }
    for ct in ["text/plain", "application/x-www-form-urlencoded", "multipart/form-data; boundary=x"] {
        let (status, _, body) = server.send("POST", "/api/grants", &[("Content-Type", ct)], grant.as_bytes());
        assert_eq!(status, 400, "Content-Type: {ct}");
        assert!(String::from_utf8_lossy(&body).contains("expected Content-Type: application/json"));
        assert_eq!(server.send("POST", "/api/import", &[("Content-Type", ct)], b"PK").0, 400, "import with Content-Type: {ct}");
    }
    assert_eq!(server.json("GET", "/api/grants", None, &[]).1["grants"], json!([]), "nothing was changed");
    assert!(!data.join("grants.json").exists());
    // The same request from this machine, as JSON, is an admin's: the refusals were the gate's.
    let (status, answer) = server.json("POST", "/api/grants", Some(serde_json::from_str(&grant).unwrap()), &[("Host", "127.0.0.1:8000")]);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(server.json("GET", "/api/grants", None, &[]).1["grants"][0]["channel"], "splunk");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// security.md Admin (#136): with ADMIN_TOKEN, the X-Admin-Token header must equal it, from every
/// address; a wrong token of any length, the same length included, is refused.
#[test]
fn claim_admin_token_must_match() {
    use serde_json::json;
    let base = tmp("claim-token");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "s", &[("a", &["splunk"])]);
    let token = "right-token-0123456789";
    let server = TestServer::start(&apps, &data, &[("ADMIN_TOKEN", token)]);
    let grant = || Some(json!({"app": "s", "channel": "splunk", "mode": "use", "decision": "allow"}));
    let same_length = "right-token-0123456780";
    assert_eq!(same_length.len(), token.len());
    let longer = format!("{token}0");
    for wrong in [None, Some(""), Some(same_length), Some("right-token"), Some(longer.as_str()), Some("RIGHT-TOKEN-0123456789")] {
        let headers: Vec<(&str, &str)> = wrong.map(|t| vec![("X-Admin-Token", t)]).unwrap_or_default();
        let (status, answer) = server.json("POST", "/api/grants", grant(), &headers);
        assert_eq!(status, 403, "token {wrong:?}: {answer}");
    }
    assert_eq!(server.json("GET", "/api/grants", None, &[]).1["grants"], json!([]), "nothing was changed");
    let (status, answer) = server.json("POST", "/api/grants", grant(), &[("X-Admin-Token", token), ("Host", "evil.com")]);
    assert_eq!(status, 200, "the right token passes, whatever the Host: {answer}");
    assert_eq!(server.json("GET", "/api/grants", None, &[]).1["grants"][0]["allow"], true);
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// security.md Capabilities, ADR-2610071219 #5, SPEC 6.9 (#120, #36, #133): reading another
/// package's table needs the user's "tables.<package>" permission, checked by the server.
#[test]
fn claim_reading_another_packages_table_needs_its_grant() {
    use serde_json::json;
    let base = tmp("claim-tables");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "owner", &[("w", &["db"])]);
    tiny_suite(&apps, "reader", &[("r", &["db"])]);
    let server = TestServer::start(&apps, &data, &[]);
    let (status, answer) = server.json("POST", "/api/db/insert", Some(json!({"package": "owner", "app": "w", "table": "t", "columns": ["a"], "rows": [[1], [2]], "create": true})), &[]);
    assert_eq!(status, 200, "{answer}");
    let read = || server.json("POST", "/api/db/page", Some(json!({"package": "reader", "app": "r", "source": "owner", "table": "t"})), &[]);
    let (status, answer) = read();
    assert_eq!(status, 400, "{answer}");
    assert!(answer["error"].as_str().unwrap_or("").contains("not allowed"), "{answer}");
    let (status, _) = server.json("POST", "/api/grants", Some(json!({"app": "reader", "channel": "tables.owner", "mode": "use", "decision": "allow"})), &[]);
    assert_eq!(status, 200);
    let (status, answer) = read();
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["total"], 2, "{answer}");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610072118 #2 (#59): a background job gets the same permission checks as a call that
/// waits; a refused one starts no job.
#[test]
fn claim_a_background_search_needs_the_grant_and_starts_no_job() {
    use serde_json::json;
    let base = tmp("claim-bg");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "s", &[("a", &["splunk", "db"]), ("quiet", &["db"])]);
    let server = TestServer::start(&apps, &data, &[]);
    for (app, why) in [("a", "you have not allowed s to use splunk"), ("quiet", "does not declare the capability \"splunk\"")] {
        for route in ["/api/splunk/search", "/api/db/search-into"] {
            let (status, answer) = server.json("POST", route, Some(json!({"package": "s", "app": app, "search": "index=x", "background": true})), &[]);
            assert_eq!(status, 400, "{route} {app}: {answer}");
            assert!(answer["error"].as_str().unwrap_or("").contains(why), "{route} {app}: {answer}");
        }
    }
    assert_eq!(server.json("GET", "/api/jobs", None, &[]).1["jobs"], json!([]), "no job was started");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610072118 #5 (#63): a long call that waits is answered with `Connection: close`; one run
/// as a background job is not.
#[test]
fn claim_a_foreground_long_call_lets_its_connection_go() {
    use serde_json::json;
    let base = tmp("claim-close");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "s", &[("a", &["splunk"])]);
    let server = TestServer::start(&apps, &data, &[]);
    server.json("POST", "/api/grants", Some(json!({"app": "s", "channel": "splunk", "mode": "use", "decision": "allow"})), &[]);
    let search = |background: bool| {
        let body = json!({"package": "s", "app": "a", "search": "index=x", "background": background}).to_string();
        server.send("POST", "/api/splunk/search", &[("Content-Type", "application/json")], body.as_bytes())
    };
    let (_, head, _) = search(false);
    assert!(head.to_ascii_lowercase().contains("connection: close"), "{head}");
    let (status, head, body) = search(true);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert!(!head.to_ascii_lowercase().contains("connection: close"), "{head}");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// SPEC 3.3, 3.4, 5.4 (#161, #162, #166): the host serves an app's files at /apps/<name>/<path>, a
/// URL ending in `/` serves that folder's index.html, and it never serves hidden files, other
/// names, or anything inside a folder named `node_modules` or `target`.
#[test]
fn claim_build_folders_and_hidden_files_are_never_served() {
    let base = tmp("claim-serve");
    let (apps, data) = (base.join("apps"), base.join("data"));
    let app = apps.join("x");
    for (rel, body) in [
        ("app.wasm", EMPTY_WASM),
        ("index.html", b"<p>top</p>".as_slice()),
        ("demo/index.html", b"<p>demo</p>"),
        ("target/a", b"build output"),
        ("target/index.html", b"<p>built</p>"),
        ("node_modules/a.js", b"downloaded"),
        ("sub/node_modules/a.js", b"downloaded"),
        ("sub/target/a", b"build output"),
        (".env", b"SECRET=1"),
        ("a b.txt", b"spaced"),
    ] {
        fs::create_dir_all(app.join(rel).parent().unwrap()).unwrap();
        fs::write(app.join(rel), body).unwrap();
    }
    let server = TestServer::start(&apps, &data, &[]);
    let get = |path: &str| server.send("GET", path, &[], b"");
    for (path, shown) in [("/apps/x/index.html", "<p>top</p>"), ("/apps/x/", "<p>top</p>"), ("/apps/x/demo/", "<p>demo</p>")] {
        let (status, _, body) = get(path);
        assert_eq!(status, 200, "{path}");
        assert!(String::from_utf8_lossy(&body).starts_with(shown), "{path}");
    }
    for path in [
        "/apps/x/target/a",
        "/apps/x/target/",
        "/apps/x/node_modules/a.js",
        "/apps/x/sub/node_modules/a.js",
        "/apps/x/sub/target/a",
        // A disk that ignores case (macOS) would find these too.
        "/apps/x/TARGET/a",
        "/apps/x/Node_Modules/a.js",
        "/apps/x/.env",
        "/apps/x/a%20b.txt",
        "/apps/x/demo/../.env",
    ] {
        assert_eq!(get(path).0, 404, "{path} must not be served");
    }
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// SPEC 7.6 (#184): a removed app moves to `.trash/<name>--<time>` inside the apps folder, leaves
/// the app list, is never served from there, and restore brings it back.
#[test]
fn claim_a_removed_app_waits_in_the_trash() {
    use serde_json::json;
    let base = tmp("claim-trash");
    let (apps, data) = (base.join("apps"), base.join("data"));
    fs::create_dir_all(apps.join("hello")).unwrap();
    fs::write(apps.join("hello/app.wasm"), EMPTY_WASM).unwrap();
    fs::write(apps.join("hello/index.html"), b"<p>hi</p>").unwrap();
    let server = TestServer::start(&apps, &data, &[]);
    assert_eq!(server.json("GET", "/api/apps", None, &[]).1, json!(["hello"]));
    let (status, answer) = server.json("POST", "/api/apps/remove", Some(json!({"name": "hello"})), &[]);
    assert_eq!(status, 200, "{answer}");
    let id = answer["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("hello--"), "{id}");
    assert!(apps.join(".trash").join(&id).join("index.html").is_file(), "moved, not deleted");
    assert!(!apps.join("hello").exists());
    assert_eq!(server.json("GET", "/api/apps", None, &[]).1, json!([]), "it leaves the app list");
    assert_eq!(server.json("GET", "/api/trash", None, &[]).1["items"][0]["id"], id.as_str());
    for path in [format!("/apps/.trash/{id}/index.html"), format!("/apps/.trash/{id}/"), format!("/frame/.trash/{id}"), "/apps/hello/index.html".into()] {
        assert_eq!(server.send("GET", &path, &[], b"").0, 404, "{path}");
    }
    let (status, answer) = server.json("POST", "/api/apps/restore", Some(json!({"id": id})), &[]);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(server.json("GET", "/api/apps", None, &[]).1, json!(["hello"]), "restore brings it back");
    assert_eq!(server.send("GET", "/apps/hello/index.html", &[], b"").0, 200);
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610072033 #3 (#50): every stop is written to wardian.log: a failed bind, and SIGTERM.
#[test]
fn claim_every_stop_is_logged() {
    let base = tmp("claim-stops");
    let apps = base.join("apps");
    fs::create_dir_all(&apps).unwrap();

    // The port is taken.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let data = base.join("bind");
    let (server, _) = TestServer::spawn(&apps, &data, &addr, &[]);
    let out = server.wait();
    let log = fs::read_to_string(data.join("wardian.log")).unwrap_or_else(|e| panic!("no wardian.log ({e}); the server said:\n{out}"));
    let last = log.lines().last().unwrap_or("");
    assert!(last.contains("stopped:") && last.contains("cannot listen on") && last.contains(&addr), "{log}");
    drop(taken);

    // SIGTERM.
    #[cfg(unix)]
    {
        let data = base.join("term");
        let server = TestServer::start(&apps, &data, &[]);
        let pid = server.child.id().to_string();
        assert!(std::process::Command::new("kill").args(["-TERM", &pid]).status().unwrap().success());
        let out = server.wait();
        let log = fs::read_to_string(data.join("wardian.log")).unwrap();
        assert!(log.lines().last().unwrap_or("").contains("stopped by SIGTERM"), "{log}\n{out}");
    }
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610072118 #1 (#56): jobs live in memory, and the server says at start that a restart
/// forgets them.
#[test]
fn claim_the_server_says_a_restart_forgets_jobs() {
    let base = tmp("claim-restart");
    let apps = base.join("apps");
    fs::create_dir_all(&apps).unwrap();
    let server = TestServer::start(&apps, &base.join("data"), &[]);
    let log = server.stop();
    assert!(log.contains("a restart forgets them"), "{log}");
    let _ = fs::remove_dir_all(base);
}

/// security.md Keys, ADR-2610071106 #3 (#141, #14): the keys, accounts, permissions and settings
/// filled in through Settings are files only their owner can read (mode 600).
#[test]
fn claim_keys_and_settings_are_private_files() {
    use serde_json::json;
    let up = fake_upstream();
    let base = tmp("claim-private");
    let (apps, data) = (base.join("apps"), base.join("data"));
    tiny_suite(&apps, "s", &[("a", &["splunk"])]);
    let server = TestServer::start(&apps, &data, &[("ANTHROPIC_BASE_URL", up.as_str()), ("WARDIAN_BEDROCK_BASE_URL", up.as_str())]);
    let post = |path: &str, v: Value| {
        let (status, answer) = server.json("POST", path, Some(v), &[]);
        assert_eq!(status, 200, "{path}: {answer}");
    };
    post("/api/ai/key", json!({"key": "sk-ant-PRIVATE", "workspace": "wrkspc_PRIVATE"}));
    post("/api/ai/provider", json!({"provider": "bedrock", "region": "us-east-1", "auth": "api-key", "token": "BEDROCK-PRIVATE"}));
    post("/api/splunk/config", json!({"url": up, "token": "SPLUNK-PRIVATE"}));
    post("/api/grants", json!({"app": "s", "channel": "splunk", "mode": "use", "decision": "allow"}));
    post("/api/source", json!({"kind": "local"}));
    server.stop();
    // A Drive key is saved only after Google accepts it, so this one is accepted by a stand-in.
    hub_with_drive(&apps, &data, Arc::new(AnyDriveKey)).set_key("{}").unwrap();
    #[cfg(unix)]
    for f in ["anthropic-key", "anthropic-workspace", "ai-provider", "bedrock.json", "splunk.json", "service-account.json", "grants.json", "config.json"] {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(data.join(f)).unwrap_or_else(|e| panic!("{f}: {e}")).permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{f} is mode {mode:o}");
    }
    let _ = fs::remove_dir_all(base);
}

/// SPEC 7.5, security.md (#147): an import refuses a name that is taken unless asked to replace
/// it, and the app already there is left exactly as it was.
#[test]
fn claim_an_import_refuses_a_taken_name() {
    use crate::usecases::import::import_zip;
    let base = tmp("claim-taken");
    let apps = base.join("apps");
    fs::create_dir_all(apps.join("hello")).unwrap();
    fs::write(apps.join("hello/app.wasm"), EMPTY_WASM).unwrap();
    fs::write(apps.join("hello/index.html"), b"<p>mine</p>").unwrap();
    let before = files_under(&base);
    let zip = zip_of(&[("hello/app.wasm", EMPTY_WASM), ("hello/index.html", b"<p>theirs</p>"), ("hello/new.js", b"//")]);
    let e = import_zip(&LocalDisk, &zip, "hello.zip", &apps, false, &|_| {}).err().expect("a taken name is refused");
    assert!(e.contains("already in the local folder: hello"), "{e}");
    assert_eq!(files_under(&base), before, "no file was added or removed");
    assert_eq!(fs::read(apps.join("hello/index.html")).unwrap(), b"<p>mine</p>");
    import_zip(&LocalDisk, &zip, "hello.zip", &apps, true, &|_| {}).unwrap();
    assert_eq!(fs::read(apps.join("hello/index.html")).unwrap(), b"<p>theirs</p>", "Replace overwrites it");
    let _ = fs::remove_dir_all(base);
}

/// SPEC 3.5, security.md (#145): a zip of more than 10,000 entries is refused, and nothing is written.
#[test]
fn claim_a_zip_of_more_than_10000_entries_is_refused() {
    use crate::domain::import_plan::MAX_ENTRIES;
    use crate::usecases::import::import_zip;
    assert_eq!(MAX_ENTRIES, 10_000);
    let base = tmp("claim-entries");
    let apps = base.join("apps");
    fs::create_dir_all(&apps).unwrap();
    let names: Vec<String> = (0..MAX_ENTRIES).map(|i| format!("hello/f{i}.txt")).collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("hello/app.wasm", EMPTY_WASM)];
    entries.extend(names.iter().map(|n| (n.as_str(), b"x".as_slice())));
    assert_eq!(entries.len(), MAX_ENTRIES + 1);
    let e = import_zip(&LocalDisk, &zip_of(&entries), "x.zip", &apps, true, &|_| {}).err().expect("refused");
    assert!(e.contains("more than 10000 entries"), "{e}");
    assert_eq!(files_under(&base), Vec::<String>::new(), "nothing was written");
    let _ = fs::remove_dir_all(base);
}

/// security.md Import links (#150): only http:// and https:// links are fetched.
#[test]
fn claim_only_http_and_https_links_are_imported() {
    let base = tmp("claim-links");
    let (apps, data) = (base.join("apps"), base.join("data"));
    fs::create_dir_all(&apps).unwrap();
    let hub = hub(&apps, &data);
    for url in ["ftp://example.com/a.zip", "file:///etc/passwd", "gopher://example.com/a.zip", "javascript:alert(1)", "//example.com/a.zip", "example.com/a.zip"] {
        let e = hub.import_url(url, false).err().unwrap_or_else(|| panic!("{url} was fetched"));
        assert!(e.contains("must start with https:// or http://"), "{url}: {e}");
    }
    let _ = fs::remove_dir_all(base);
}

/// SPEC 2.2 (#157): a package in a newer format is listed with the reason it does not run.
#[test]
fn claim_a_newer_format_says_update_wardian() {
    let base = tmp("claim-format");
    let (apps, data) = (base.join("apps"), base.join("data"));
    for (name, file, body) in [("mod", "app.json", r#"{"format": 99, "title": "New"}"#), ("st", "suite.json", r#"{"format": 99, "title": "New", "apps": []}"#)] {
        fs::create_dir_all(apps.join(name)).unwrap();
        fs::write(apps.join(name).join(file), body).unwrap();
        fs::write(apps.join(name).join("app.wasm"), EMPTY_WASM).unwrap();
    }
    fs::create_dir_all(apps.join("old")).unwrap();
    fs::write(apps.join("old/app.wasm"), EMPTY_WASM).unwrap();
    let listed = hub(&apps, &data).apps();
    for name in ["mod", "st"] {
        let info = listed.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("{name} is listed"));
        let e = info.error.as_deref().unwrap_or("");
        assert!(e.contains("Update Wardian") && e.contains("format 99"), "{name}: {e:?}");
    }
    assert!(listed.iter().find(|a| a.name == "old").unwrap().error.is_none());
    let _ = fs::remove_dir_all(base);
}

/// SPEC 5.1 (#164): `app.wasm` must be a WebAssembly binary, version 1; `wardian check` says so.
#[test]
fn claim_app_wasm_must_be_webassembly_version_1() {
    let dir = tmp("claim-wasm").join("w");
    fs::create_dir_all(&dir).unwrap();
    for (bytes, why) in [(b"\0asm\x02\0\0\0".as_slice(), "need version 1"), (b"\0asm", "need version 1"), (b"MZ\x90\0\x03\0\0\0", "not a WebAssembly module"), (b"", "not a WebAssembly module")] {
        fs::write(dir.join("app.wasm"), bytes).unwrap();
        let (ok, report) = checker().check_dir(&dir, "w");
        assert!(!ok && report.contains(why), "{bytes:?}: {report}");
    }
    fs::write(dir.join("app.wasm"), EMPTY_WASM).unwrap();
    assert!(checker().check_dir(&dir, "w").0);
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

/// SPEC 7.2 (#182): a `.zip`, a `.wardian` and a `.rustle` file are read the same way, by the
/// importer and by `wardian check`.
#[test]
fn claim_zip_wardian_and_rustle_files_are_accepted() {
    use crate::usecases::import::import_zip;
    let base = tmp("claim-rustle");
    let zip = zip_of(&[("app.wasm", EMPTY_WASM), ("index.html", b"<p>hi</p>")]);
    for ext in ["zip", "wardian", "rustle"] {
        let apps = base.join(ext);
        let done = import_zip(&LocalDisk, &zip, &format!("my-app.{ext}"), &apps, false, &|_| {}).unwrap();
        assert_eq!(done.apps, ["my-app"], ".{ext}: named after the file");
        let file = base.join(format!("my-app.{ext}"));
        fs::write(&file, &zip).unwrap();
        let (ok, report) = checker().check_path(&file);
        assert!(ok && report.join("").contains("my-app"), ".{ext}: {report:?}");
    }
    let _ = fs::remove_dir_all(base);
}

/// SPEC 7.3 (#183): the importer accepts project zips as they come, by the listed rules.
#[test]
fn claim_import_is_lenient() {
    use crate::usecases::import::import_zip;
    let base = tmp("claim-lenient");
    let n = std::cell::Cell::new(0);
    let import = |entries: &[(&str, &[u8])], zip_name: &str| {
        n.set(n.get() + 1);
        let apps = base.join(format!("t{}", n.get()));
        let done = import_zip(&LocalDisk, &zip_of(entries), zip_name, &apps, false, &|_| {}).unwrap();
        (done, apps)
    };
    let w = EMPTY_WASM;
    // A folder holding suite.json is a package top, with everything inside it.
    let (done, apps) = import(&[("proj/my-suite/suite.json", b"{}"), ("proj/my-suite/apps/a/app.js", b"//"), ("proj/my-suite/apps/a/x.wasm", w)], "x.zip");
    assert_eq!(done.apps, ["my-suite"]);
    assert!(apps.join("my-suite/apps/a/app.js").is_file() && apps.join("my-suite/apps/a/x.wasm").is_file());
    // The nearest folder above a .wasm that holds app.json is the top.
    let (done, apps) = import(&[("calc/app.json", b"{}"), ("calc/build/out/calc.wasm", w)], "x.zip");
    assert_eq!(done.apps, ["calc"]);
    assert!(apps.join("calc/app.wasm").is_file(), "the one .wasm is also saved as app.wasm");
    // Otherwise the .wasm's folder, skipping build folders.
    let (done, apps) = import(&[("usl-wasm/pkg/usl_wasm.wasm", w), ("usl-wasm/pkg/usl_wasm.js", b"//")], "x.zip");
    assert_eq!(done.apps, ["usl-wasm"]);
    assert!(apps.join("usl-wasm/app.wasm").is_file() && apps.join("usl-wasm/pkg/usl_wasm.wasm").is_file());
    for build in ["dist", "build", "out", "output", "release", "wasm", "bin", "www", "public"] {
        let path = format!("tool/{build}/t.wasm");
        assert_eq!(import(&[(path.as_str(), w)], "x.zip").0.apps, ["tool"], "{build}/ is skipped");
    }
    // A package at the very top: named after its .wasm file, or after the zip for app.wasm.
    assert_eq!(import(&[("adder.wasm", w)], "bundle.zip").0.apps, ["adder"]);
    assert_eq!(import(&[("app.wasm", w)], "bundle.zip").0.apps, ["bundle"]);
    // Several .wasm files and no app.wasm: skipped, as is anything outside every package top.
    let (done, _) = import(&[("two/a.wasm", w), ("two/b.wasm", w), ("one/app.wasm", w), ("README.md", b"x")], "x.zip");
    assert_eq!(done.apps, ["one"]);
    assert!(done.skipped.iter().any(|s| s.contains("several .wasm files")) && done.skipped.iter().any(|s| s.contains("outside any app folder")), "{:?}", done.skipped);
    let _ = fs::remove_dir_all(base);
}

/// SPEC 8 (#186): `wardian check` exits 0 when no package has errors, warnings included, and 1
/// otherwise.
#[test]
fn claim_check_exits_0_or_1_and_warnings_do_not_fail() {
    use crate::adapters::primary::cli::{run, Command};
    let base = tmp("claim-exit");
    let warned = base.join("warned");
    fs::create_dir_all(&warned).unwrap();
    fs::write(warned.join("app.wasm"), EMPTY_WASM).unwrap();
    fs::write(warned.join("app.json"), r#"{"format": 1, "titel": "a typo"}"#).unwrap();
    let (ok, report) = checker().check_dir(&warned, "warned");
    assert!(ok && report.contains("warn"), "a warning: {report}");
    let broken = base.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("app.wasm"), b"not wasm").unwrap();
    let docs = Docs::new(Arc::new(Embedded));
    let no_exports = || -> Arc<dyn crate::ports::service::Exports> { unreachable!("check does not export") };
    let exit = |paths: &[&Path]| {
        let mut args = vec!["check".to_string()];
        args.extend(paths.iter().map(|p| p.display().to_string()));
        match run(&args, &scaffold(), &docs, &base, &no_exports) {
            Command::Exit(code) => code,
            Command::Serve { .. } => panic!("check does not serve"),
        }
    };
    assert_eq!(exit(&[&warned]), 0);
    assert_eq!(exit(&[&broken]), 1);
    assert_eq!(exit(&[&warned, &broken]), 1, "one failing package fails the check");
    let _ = fs::remove_dir_all(base);
}
