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
        .filter(|(rel, body)| fs::read(Path::new("website").join(rel)).ok().as_ref() != Some(body))
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
        fn example_apps(&self) -> &'static [(&'static str, &'static [u8])] {
            Embedded.example_apps()
        }
        fn host_file(&self, name: &str) -> Option<&'static str> {
            Embedded.host_file(name)
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

/// ADR-2610081830: the folders are kept in state/folders.json, private; the examples are filed
/// once; what a viewer sends is checked and tidied; the host decides `seeded`.
#[test]
fn folders_kept_in_the_state_folder() {
    use crate::ports::service::ViewerState;
    use crate::usecases::viewer_state::State;
    use serde_json::json;
    let dir = tmp("folders");
    let state = State::new(Arc::new(LocalDisk), &dir).with_examples(vec!["adder".into(), "usl-lab".into()]);
    let apps: Vec<String> = vec!["adder".into(), "mine".into(), "usl-lab".into()];

    let f = state.folders(&apps).unwrap();
    assert_eq!(f["folders"][0]["name"], "Examples", "{f}");
    assert_eq!(f["folders"][0]["apps"], json!(["adder", "usl-lab"]));
    assert_eq!((f["folders"][0]["open"].as_bool(), f["seeded"].as_bool()), (Some(false), Some(true)));

    let mine = json!({"folders": [{"id": "work", "name": " Work ", "open": true, "apps": ["mine", "gone"]}], "seeded": false});
    let kept = state.set_folders(&mine, &apps).unwrap();
    assert_eq!(kept["folders"], json!([{"id": "work", "name": "Work", "open": true, "apps": ["mine"]}]));
    assert_eq!(kept["seeded"], true, "the viewer cannot ask for the examples to be filed again");
    let again = state.folders(&apps).unwrap();
    assert_eq!(again["folders"].as_array().unwrap().len(), 1, "deleting Examples keeps it deleted: {again}");
    assert!(state.set_folders(&json!({"folders": [{"id": "x", "name": ""}]}), &apps).is_err());
    assert_eq!(state.folders(&apps).unwrap(), again, "a refused change keeps what was there");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(dir.join("state/folders.json")).unwrap().permissions().mode() & 0o777, 0o600);
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
    let seed = |disk: &LocalDisk, source: &Path, working: &Path| crate::usecases::workspace::add_examples(disk, Some(source), &[], working).map(|n| (!n.is_empty()).then_some(n.len()));
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

    // The examples are added once, without build output, and the source is left alone.
    assert_eq!(workspace::add_examples(&disk, Some(&source), &[], &working).unwrap(), ["hello"]);
    assert!(disk.is_file(&working.join("hello/index.html")));
    assert!(!disk.exists(&working.join("hello/target")) && !disk.exists(&working.join("hello/Cargo.lock")), "build output is not copied");
    assert!(workspace::add_examples(&disk, Some(&source), &[], &working).unwrap().is_empty(), "an example already given is not added again");

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
    let (secrets, checks) = key_stores(&dir);
    let splunk = Splunk::new(secrets, checks, Arc::new(Arc::clone(&fake)), Arc::clone(&db), &dir, Some(cfg));
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
    fn agent(&self) -> Value { Value::Null }
    fn set_agent(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn usage(&self) -> Value { Value::Null }
}

/// Secrets as the server keeps them, sealed over the real disk, with their tests recorded in `dir`.
fn key_stores(dir: &Path) -> (Arc<dyn crate::ports::secrets::Secrets>, Arc<crate::usecases::keys::KeyChecks>) {
    use crate::adapters::secondary::sealed_secrets::SealedSecrets;
    (Arc::new(SealedSecrets::with_key(Arc::new(LocalDisk), [3; 32])), Arc::new(crate::usecases::keys::KeyChecks::new(Arc::new(LocalDisk), dir)))
}

/// The master key every test server seals with, so no test touches the user's key file (ADR-2610081501).
const TEST_MASTER_KEY: &str = "0101010101010101010101010101010101010101010101010101010101010101";

fn job_runner(tag: &str, rows: usize) -> (crate::usecases::jobs::JobRunner, Arc<SlowSplunk>, std::path::PathBuf) {
    use crate::adapters::secondary::sqlite_store::SqliteStore;
    use crate::ports::db::Database;
    use crate::usecases::{jobs::JobRunner, splunk::Splunk};
    let dir = tmp(tag);
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&dir));
    let fake = Arc::new(SlowSplunk { rows, done: Default::default(), cancelled: Default::default() });
    let cfg = crate::ports::splunk::SplunkConfig { url: "https://splunk.test:8089".into(), token: "t".into(), ..Default::default() };
    let (secrets, checks) = key_stores(&dir);
    let splunk = Splunk::new(secrets, checks, Arc::new(Arc::clone(&fake)), db, &dir, Some(cfg));
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
    // "-" names no apps folder, so Wardian serves its working folder; WARDIAN_TEST_CWD starts it elsewhere.
    if let Ok(dir) = std::env::var("WARDIAN_TEST_CWD") {
        std::env::set_current_dir(dir).unwrap();
    }
    let apps = (apps != "-").then_some(apps);
    crate::serve(crate::config::Settings::from_env(apps.as_deref(), &LocalDisk), true);
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
            .env("WARDIAN_MASTER_KEY", TEST_MASTER_KEY)
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
    for path in ["/api/status", "/api/grants", "/api/apps", "/api/app-list", "/api/trash", "/api/ai/sessions", "/api/history/loan-planner", "/api/state/apps/loan-planner", "/api/state/layout/loan-planner", "/api/apps/loan-planner/export?data=1&preview=1", "/api/keys", "/api/agent", "/api/usage"] {
        ask("GET", path, None, true);
    }
    // The key list's own answers (ADR-2610081500): each test, with the refusals that name a key.
    for id in ["anthropic", "bedrock", "splunk", "drive", "admin", "nothing"] {
        post(&format!("/api/keys/{id}/test"), json!({}));
    }
    post("/api/agent", json!({ "models": { "bedrock": { "main": "us.anthropic.chosen-v1:0" } }, "caps": { "loan-planner": 1000 } }));
    post("/api/keys/admin", json!({ "token": "short" }));
    for path in ["/api/status", "/api/grants", "/api/apps", "/api/app-list", "/api/ai/sessions", "/api/drive/browse", "/api/keys", "/api/agent", "/api/usage"] {
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
    assert!(log.contains("admin: whoever sends the admin token") && log.contains("export: loan-planner with its data"), "the server's log was read:\n{log}");

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
    match crate::take_address(&format!("127.0.0.1:{port}"), false, port.saturating_add(5), "apps") {
        crate::Address::Ready(listener, busy, other) => {
            assert_eq!(other, None, "a plain listener is not named as a Wardian");
            let got = listener.local_addr().unwrap().port();
            assert!(got > port && got <= port.saturating_add(5), "took {got}, held {port}");
            assert_eq!(busy, Some(port));
        }
        crate::Address::Running(at) => panic!("a plain listener is not a Wardian: {at}"),
        crate::Address::Failed(what, todo) => panic!("{what} {todo}"),
    }
    // With ADDR set, the same busy port is an error that says what to do.
    match crate::take_address(&format!("127.0.0.1:{port}"), true, port.saturating_add(5), "apps") {
        crate::Address::Failed(what, todo) => {
            assert!(what.contains(&format!("cannot listen on 127.0.0.1:{port}")), "{what}");
            assert!(todo.contains("ADDR="), "{todo}");
        }
        _ => panic!("ADDR set: a busy port must be an error"),
    }
    drop(held);
}

/// A fake Wardian on a free port that answers `/api/status` with `body` a few times.
fn fake_wardian(body: &'static str) -> u16 {
    use std::io::{Read, Write};
    let fake = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = fake.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in fake.incoming().flatten().take(4) {
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    port
}

/// ADR-2610080930: with ADDR unset, this same Wardian (version and apps folder) answering on the
/// port is reported as running, and nothing is taken.
#[test]
fn start_finds_the_same_wardian_already_running() {
    let body: &'static str = Box::leak(format!(r#"{{"source":"local","version":"{}","local_root":"apps","apps":5,"first_run":false,"admin":true}}"#, env!("CARGO_PKG_VERSION")).into_boxed_str());
    let port = fake_wardian(body);
    match crate::take_address(&format!("127.0.0.1:{port}"), false, port.saturating_add(5), "apps") {
        crate::Address::Running(at) => assert_eq!(at, format!("127.0.0.1:{port}")),
        crate::Address::Ready(l, _, _) => panic!("took {:?} although this Wardian runs on {port}", l.local_addr()),
        crate::Address::Failed(what, todo) => panic!("{what} {todo}"),
    }
}

/// An older Wardian on the port, or one serving another folder, is not opened: this one takes the
/// next free port and says which Wardian holds the usual one.
#[test]
fn start_passes_over_another_wardian_and_names_it() {
    for (body, says) in [
        (r#"{"source":"local","local_root":"data/apps","apps":0,"first_run":false}"#, "an older Wardian serving data/apps holds port"),
        (r#"{"source":"local","version":"0.0.1","local_root":"apps","apps":5,"first_run":false}"#, "Wardian 0.0.1 serving apps holds port"),
    ] {
        let port = fake_wardian(body);
        match crate::take_address(&format!("127.0.0.1:{port}"), false, port.saturating_add(5), "apps") {
            crate::Address::Ready(listener, busy, other) => {
                assert!(listener.local_addr().unwrap().port() > port);
                assert_eq!(busy, Some(port));
                let other = other.expect("the other Wardian is named");
                assert!(other.contains(says) && other.contains("Ctrl-C"), "{other}");
            }
            crate::Address::Running(at) => panic!("opened another Wardian at {at}"),
            crate::Address::Failed(what, todo) => panic!("{what} {todo}"),
        }
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
    let (secrets, checks) = key_stores(data);
    let ports = HubPorts { fs: Arc::clone(&fs), drive, web: Arc::new(LinkFetcher::new(false)), assets: Arc::new(Embedded), secrets };
    let history = Arc::new(History::new(fs, data, apps));
    Hub::new(ports, checks, history, data.to_path_buf(), apps.to_path_buf(), std::time::Duration::from_secs(60))
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
    tiny_suite(&apps, "s", &[("a", &["splunk", "claude:sample"])]);
    let server = TestServer::start(&apps, &data, &[("ANTHROPIC_BASE_URL", up.as_str()), ("WARDIAN_BEDROCK_BASE_URL", up.as_str())]);
    let post = |path: &str, v: Value| {
        let (status, answer) = server.json("POST", path, Some(v), &[]);
        assert_eq!(status, 200, "{path}: {answer}");
    };
    // keys.md (ADR-2610081500): Claude's settings, the usage counts, the key tests and the admin token.
    post("/api/agent", json!({"max_steps": 41}));
    post("/api/ai/key", json!({"key": "sk-ant-PRIVATE", "workspace": "wrkspc_PRIVATE"}));
    post("/api/ai/provider", json!({"provider": "bedrock", "region": "us-east-1", "auth": "api-key", "token": "BEDROCK-PRIVATE"}));
    post("/api/splunk/config", json!({"url": up, "token": "SPLUNK-PRIVATE"}));
    post("/api/grants", json!({"app": "s", "channel": "splunk", "mode": "use", "decision": "allow"}));
    post("/api/source", json!({"kind": "local"}));
    post("/api/grants", json!({"app": "s", "channel": "ai", "mode": "use", "decision": "allow"}));
    post("/api/ai/sample", json!({"package": "s", "app": "a", "prompt": "hi"}));
    post("/api/keys/admin", json!({"token": "a-saved-admin-token-0123456789"}));
    server.stop();
    // A Drive key is saved only after Google accepts it, so this one is accepted by a stand-in.
    hub_with_drive(&apps, &data, Arc::new(AnyDriveKey)).set_key("{}").unwrap();
    #[cfg(unix)]
    for f in ["anthropic-key", "anthropic-workspace", "ai-provider", "bedrock.json", "splunk.json", "service-account.json", "grants.json", "config.json",
              "agent.json", "usage.json", "key-checks.json", "admin-token"] {
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
    let no_service = || -> Box<dyn crate::ports::service_manager::Background> { unreachable!() };
    let exit = |paths: &[&Path]| {
        let mut args = vec!["check".to_string()];
        args.extend(paths.iter().map(|p| p.display().to_string()));
        let no_key = || -> Box<dyn crate::ports::secrets::MasterKey> { unreachable!("check does not touch the key") };
        match run(&args, &scaffold(), &docs, &base, &no_exports, &no_key, &no_service) {
            Command::Exit(code) => code,
            Command::Serve { .. } => panic!("check does not serve"),
        }
    };
    assert_eq!(exit(&[&warned]), 0);
    assert_eq!(exit(&[&broken]), 1);
    assert_eq!(exit(&[&warned, &broken]), 1, "one failing package fails the check");
    let _ = fs::remove_dir_all(base);
}

// ---------- Keys, Claude's settings and usage (ADR-2610081500) ----------

/// A Claude that answers every request with 100 tokens in and 50 out, and keeps what it was asked.
#[derive(Default)]
struct FakeClaude {
    bodies: std::sync::Mutex<Vec<Value>>,
    tested: std::sync::Mutex<Vec<String>>,
    refuse: std::sync::atomic::AtomicBool,
}

impl crate::ports::llm::Llm for FakeClaude {
    fn model(&self, tier: crate::ports::llm::Tier) -> String {
        match tier {
            crate::ports::llm::Tier::Main => "fake-main".into(),
            crate::ports::llm::Tier::Quick => "fake-quick".into(),
        }
    }
    fn messages(&self, _: &crate::ports::llm::LlmAuth, body: &Value) -> Result<Value, crate::ports::llm::LlmError> {
        self.bodies.lock().unwrap().push(body.clone());
        if self.refuse.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::ports::llm::LlmError::Status(401, "invalid x-api-key".into()));
        }
        Ok(serde_json::json!({ "content": [{ "type": "text", "text": "hello" }], "stop_reason": "end_turn",
                               "usage": { "input_tokens": 100, "output_tokens": 50 } }))
    }
    fn test_key(&self, auth: &crate::ports::llm::LlmAuth, model: &str) -> Result<(), crate::ports::llm::LlmError> {
        self.tested.lock().unwrap().push(model.into());
        if model == "nope" {
            return Err(crate::ports::llm::LlmError::Status(404, format!("model: {model}")));
        }
        match auth {
            crate::ports::llm::LlmAuth::Anthropic { key, .. } if key.starts_with("bad") => Err(crate::ports::llm::LlmError::Status(401, "invalid x-api-key".into())),
            _ => Ok(()),
        }
    }
}

/// The studio as the server builds it, over a fake Claude for both providers.
fn studio_on(fake: &Arc<FakeClaude>, base: &Path) -> (crate::usecases::studio::Studio, Arc<crate::usecases::keys::KeyChecks>, std::path::PathBuf) {
    use crate::usecases::{studio::{Providers, Stores, Studio}, usage::Meter};
    let (apps, data) = (base.join("apps"), base.join("data"));
    fs::create_dir_all(&apps).unwrap();
    fs::create_dir_all(&data).unwrap();
    let (secrets, checks) = key_stores(&data);
    let disk: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let stores = Stores { fs: Arc::clone(&disk), secrets, checks: Arc::clone(&checks), meter: Arc::new(Meter::new(Arc::clone(&disk), &data)) };
    let providers = Providers { anthropic: fake.clone(), bedrock: fake.clone(), anthropic_key: None, anthropic_workspace: None, provider: None, bedrock_env: None };
    let studio = Studio::new(stores, providers, Arc::new(Embedded), Arc::new(hub(&apps, &data)), Arc::new(Checker::new(disk)), &data);
    (studio, checks, data)
}

const GOOD_KEY: &str = "sk-ant-test-0123456789-abcd";

/// ADR-2610081500 #5 and #6: each app's `claude:sample` tokens are counted, by day and by app,
/// and past its cap the app gets `over_budget` without a request being sent. Another app is not
/// stopped, and the counts survive a restart.
#[test]
fn usage_sample_is_counted_per_app_and_stopped_at_its_cap() {
    use serde_json::json;
    let base = tmp("usage-cap");
    let fake = Arc::new(FakeClaude::default());
    let (studio, _, data) = studio_on(&fake, &base);
    studio.set_key(GOOD_KEY, None).unwrap();
    studio.set_agent(&json!({ "caps": { "notes": 300 } })).unwrap();
    let ask = |package: &str| studio.sample(&json!({ "package": package, "prompt": "hi" }));
    assert_eq!(ask("notes").unwrap()["text"], "hello");
    assert!(ask("notes").is_ok(), "150 of 300 used");
    let sent = fake.bodies.lock().unwrap().len();
    let refused = ask("notes").unwrap_err();
    assert!(refused.starts_with("over_budget: ") && refused.contains("300"), "{refused}");
    assert_eq!(fake.bodies.lock().unwrap().len(), sent, "no request is sent past the cap");
    assert!(ask("other").is_ok(), "another app has its own cap");
    let usage = studio.usage();
    assert_eq!(usage["totals"]["notes"]["total"], 300, "{usage}");
    assert_eq!(usage["totals"]["notes"]["requests"], 2);
    assert_eq!(usage["caps"]["notes"], 300);
    assert_eq!(usage["caps"]["other"], 200_000, "the default cap");
    let again = crate::usecases::usage::Meter::new(Arc::new(LocalDisk), &data);
    assert_eq!(again.used_today("notes"), 300, "usage.json keeps the count");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #6: Make an app stops a turn at its daily cap with a message, before it asks Claude.
#[test]
fn usage_make_an_app_stops_at_its_cap() {
    use crate::domain::agent::BUILDER;
    use serde_json::json;
    let base = tmp("usage-build");
    let fake = Arc::new(FakeClaude::default());
    let (studio, _, data) = studio_on(&fake, &base);
    studio.set_key(GOOD_KEY, None).unwrap();
    studio.set_agent(&json!({ "build_daily_tokens": 100 })).unwrap();
    crate::usecases::usage::Meter::new(Arc::new(LocalDisk), &data).record(BUILDER, &json!({ "usage": { "input_tokens": 100 } }));
    // A fresh studio reads the count the meter above saved.
    let (studio, _, _) = studio_on(&fake, &base);
    studio.set_key(GOOD_KEY, None).unwrap();
    let id = studio.send(&json!({ "message": "a habit tracker" })).unwrap()["session"].as_str().unwrap().to_string();
    let t = std::time::Instant::now();
    let events = loop {
        let e = studio.events(&id, 0).unwrap();
        if e["busy"] == false || t.elapsed() > std::time::Duration::from_secs(20) {
            break e;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let last = events["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["kind"], "error", "{events}");
    assert!(last["text"].as_str().unwrap().contains("Make an app used its 100 tokens for today"), "{last}");
    assert!(fake.bodies.lock().unwrap().is_empty(), "Claude was never asked");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #4: a model chosen in Settings is tested before it is saved; a model that fails
/// is not saved; the saved one is the one each request names; the limits reach the requests; and a
/// model for a provider with no credentials is saved untested, and the answer says so.
#[test]
fn agent_models_are_tested_before_they_are_saved_and_reach_the_requests() {
    use serde_json::json;
    let base = tmp("agent-models");
    let fake = Arc::new(FakeClaude::default());
    let (studio, _, data) = studio_on(&fake, &base);
    studio.set_key(GOOD_KEY, None).unwrap();
    assert_eq!(*fake.tested.lock().unwrap(), ["fake-main"], "a key is tested with the main model");

    let refused = studio.set_agent(&json!({ "models": { "anthropic": { "main": "nope" } } })).unwrap_err();
    assert!(refused.contains("nope") && refused.contains("404"), "{refused}");
    assert_eq!(studio.agent()["models"]["anthropic"]["main"], "fake-main", "a model that fails is not saved");

    let out = studio.set_agent(&json!({ "models": { "anthropic": { "main": "claude-chosen", "quick": "claude-small" } }, "sample_max_tokens": 300 })).unwrap();
    assert_eq!(out["untested"], json!([]));
    assert_eq!(fake.tested.lock().unwrap()[2..], ["claude-chosen", "claude-small"]);
    studio.sample(&json!({ "package": "a", "prompt": "hi" })).unwrap();
    studio.sample(&json!({ "package": "a", "prompt": "hi", "tier": "quick" })).unwrap();
    let bodies = fake.bodies.lock().unwrap().clone();
    assert_eq!((bodies[0]["model"].as_str(), bodies[0]["max_tokens"].as_u64()), (Some("claude-chosen"), Some(300)));
    assert_eq!(bodies[1]["model"], "claude-small");

    let out = studio.set_agent(&json!({ "models": { "bedrock": { "main": "us.anthropic.x-v1:0" } } })).unwrap();
    assert_eq!(out["untested"], json!(["us.anthropic.x-v1:0"]), "Bedrock has no credentials to test with");

    let saved: Value = serde_json::from_slice(&fs::read(data.join("agent.json")).unwrap()).unwrap();
    assert_eq!(saved["models"]["anthropic"]["main"], "claude-chosen");
    assert!(studio.set_agent(&json!({ "max_steps": 1 })).is_err(), "out of bounds");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #2: a key Claude refuses while an app uses it is recorded, so the key list says
/// so before anyone opens a log.
#[test]
fn keys_a_key_refused_while_an_app_runs_is_recorded() {
    use crate::usecases::keys::KeyOwner;
    use serde_json::json;
    let base = tmp("keys-refused");
    let fake = Arc::new(FakeClaude::default());
    let (studio, checks, _) = studio_on(&fake, &base);
    studio.set_key(GOOD_KEY, None).unwrap();
    assert_eq!(checks.last("anthropic")["ok"], true);
    fake.refuse.store(true, std::sync::atomic::Ordering::Relaxed);
    let e = studio.sample(&json!({ "package": "notes", "prompt": "hi" })).unwrap_err();
    assert!(e.starts_with("not_granted: "), "{e}");
    let check = checks.last("anthropic");
    assert_eq!(check["ok"], false);
    assert!(check["said"].as_str().unwrap().contains("refused while notes used it (HTTP 401)"), "{check}");
    let entry = &studio.entries()[0];
    assert_eq!((entry.id, entry.from), ("anthropic", Some("settings")));
    assert!(entry.detail.contains("key ending abcd") && !entry.detail.contains(GOOD_KEY), "{}", entry.detail);
    assert_eq!(studio.retest("anthropic"), Some(Ok(())), "test again: the fake's test_key still accepts the key");
    assert_eq!(checks.last("anthropic")["ok"], true);
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #2: every secret is on the list with where it came from; "test again" tests
/// the saved values and records the answer; removing a saved secret falls back to the
/// environment's; the Drive key, which could not be removed before, can be.
#[test]
fn keys_list_test_again_and_remove() {
    use serde_json::json;
    let up = fake_upstream();
    let base = tmp("keys-list");
    let (data, apps) = (base.join("data"), base.join("apps"));
    fs::create_dir_all(&apps).unwrap();
    let env = [("ANTHROPIC_API_KEY", "sk-ant-from-the-environment-wxyz"), ("ANTHROPIC_BASE_URL", up.as_str()), ("GDRIVE_API_BASE", up.as_str())];
    let server = TestServer::start(&apps, &data, &env);
    let keys = |s: &TestServer| s.json("GET", "/api/keys", None, &[]).1;
    let entry = |v: &Value, id: &str| v["keys"].as_array().unwrap().iter().find(|k| k["id"] == id).cloned().unwrap();

    let list = keys(&server);
    let ids: Vec<&str> = list["keys"].as_array().unwrap().iter().map(|k| k["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["anthropic", "bedrock", "splunk", "drive", "admin"]);
    assert_eq!(entry(&list, "anthropic")["from"], "environment");
    assert_eq!(entry(&list, "anthropic")["removable"], false, "an environment key is not removable in Settings");
    assert_eq!(entry(&list, "drive")["set"], false);
    assert_eq!(list["kept"]["sealed"], true);
    assert_eq!(list["kept"]["master"], "WARDIAN_MASTER_KEY");

    let (st, _) = server.json("POST", "/api/ai/key", Some(json!({ "key": "sk-ant-typed-in-settings-0123" })), &[]);
    assert_eq!(st, 200);
    let list = keys(&server);
    let anthropic = entry(&list, "anthropic");
    assert_eq!((anthropic["from"].as_str(), anthropic["check"]["ok"].as_bool()), (Some("settings"), Some(true)), "{anthropic}");
    assert!(anthropic["detail"].as_str().unwrap().contains("key ending 0123"), "{anthropic}");

    let (st, tested) = server.json("POST", "/api/keys/anthropic/test", Some(json!({})), &[]);
    assert_eq!((st, tested["tested"]["ok"].as_bool()), (200, Some(true)), "{tested}");
    let (st, none) = server.json("POST", "/api/keys/splunk/test", Some(json!({})), &[]);
    assert_eq!((st, none["tested"]["ok"].as_bool()), (200, Some(false)), "a failed test is an answer: {none}");
    assert_eq!(none["tested"]["said"], "no Splunk account is set");
    let (st, _) = server.json("POST", "/api/keys/admin/test", Some(json!({})), &[]);
    assert_eq!(st, 400);
    let (st, _) = server.json("POST", "/api/keys/nothing/remove", Some(json!({})), &[]);
    assert_eq!(st, 400);

    let (st, after) = server.json("POST", "/api/keys/anthropic/remove", Some(json!({})), &[]);
    assert_eq!(st, 200);
    assert_eq!(entry(&after, "anthropic")["from"], "environment", "the environment's key applies again");
    assert!(entry(&after, "anthropic")["check"].is_null(), "the removed key's test is forgotten");
    assert!(!data.join("anthropic-key").exists());

    let sa = json!({ "type": "service_account", "client_email": "w@example.iam.gserviceaccount.com", "token_uri": format!("{up}/token"),
                     "private_key": "-----BEGIN PRIVATE KEY-----\nMII\n-----END PRIVATE KEY-----\n" });
    let (st, body) = server.json("POST", "/api/drive/key", Some(sa), &[]);
    if st == 200 {
        assert_eq!(entry(&keys(&server), "drive")["from"], "settings");
        let (st, after) = server.json("POST", "/api/keys/drive/remove", Some(json!({})), &[]);
        assert_eq!((st, entry(&after, "drive")["set"].as_bool()), (200, Some(false)), "{after}");
        assert!(!data.join("service-account.json").exists());
    } else {
        // The stand-in key is not a real RSA key; the Drive owner's removal is tested below.
        assert!(body["error"].is_string(), "{body}");
    }
    assert_eq!(server.json("GET", "/api/keys", None, &[("Host", "evil.com")]).0, 403, "only an admin sees the list");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #2: the Drive key, saved through Settings, can be removed, and its tests recorded.
#[test]
fn keys_the_drive_key_can_be_removed() {
    use crate::usecases::keys::KeyOwner;
    let base = tmp("keys-drive");
    let (apps, data) = (base.join("apps"), base.join("data"));
    fs::create_dir_all(&apps).unwrap();
    let hub = hub_with_drive(&apps, &data, Arc::new(AnyDriveKey));
    hub.start(None, None);
    assert_eq!(hub.entries()[0].from, None);
    hub.set_key("{}").unwrap();
    assert_eq!(hub.entries()[0].from, Some("settings"));
    assert!(hub.entries()[0].detail.contains("w@example.iam.gserviceaccount.com"));
    assert_eq!(hub.retest("drive"), Some(Ok(())));
    assert_eq!(hub.forget("drive"), Some(Ok(())));
    assert_eq!(hub.entries()[0].from, None);
    assert!(!data.join("service-account.json").exists());
    assert!(hub.retest("drive").unwrap().is_err(), "nothing left to test");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081500 #3: an admin token set in Settings locks every admin request at once and after a
/// restart; a short one is refused; ADMIN_TOKEN wins over it; removing it unlocks this machine
/// again; and while Wardian listens on a non-loopback address it cannot be removed.
#[test]
fn keys_admin_token_set_in_settings() {
    use serde_json::json;
    let base = tmp("keys-admin");
    let (data, apps) = (base.join("data"), base.join("apps"));
    fs::create_dir_all(&apps).unwrap();
    let server = TestServer::start(&apps, &data, &[]);
    assert_eq!(server.json("POST", "/api/keys/admin", Some(json!({ "token": "too-short" })), &[]).0, 400);
    assert_eq!(server.json("POST", "/api/keys/admin", Some(json!({ "token": "has spaces in it, twenty-four+" })), &[]).0, 400);
    let (st, set) = server.json("POST", "/api/keys/admin", Some(json!({ "generate": true })), &[]);
    assert_eq!((st, set["generated"].as_bool()), (200, Some(true)), "{set}");
    let token = set["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 64);
    let with = [("X-Admin-Token", token.as_str())];
    assert_eq!(server.json("GET", "/api/keys", None, &[]).0, 403, "this machine is no longer an admin without the token");
    assert_eq!(server.json("POST", "/api/grants", Some(json!({})), &[]).0, 403);
    let (st, list) = server.json("GET", "/api/keys", None, &with);
    assert_eq!(st, 200);
    let admin = list["keys"].as_array().unwrap().iter().find(|k| k["id"] == "admin").unwrap().clone();
    assert_eq!((admin["from"].as_str(), admin["removable"].as_bool()), (Some("settings"), Some(true)));
    assert!(!list.to_string().contains(&token), "the list never shows the token");
    let log = server.stop();
    assert!(!log.contains(&token), "the token is not printed");

    let server = TestServer::start(&apps, &data, &[]);
    assert_eq!(server.json("GET", "/api/keys", None, &[]).0, 403, "still locked after a restart");
    assert_eq!(server.json("GET", "/api/keys", None, &with).0, 200);
    let (st, _) = server.json("POST", "/api/keys/admin/remove", Some(json!({})), &with);
    assert_eq!(st, 200);
    assert_eq!(server.json("GET", "/api/keys", None, &[]).0, 200, "this machine is an admin again");
    assert_eq!(server.json("POST", "/api/keys/admin", Some(json!({ "token": token })), &[]).0, 200, "a typed token is saved");
    server.stop();

    let env_token = "from-the-environment-0123456789";
    let server = TestServer::start(&apps, &data, &[("ADMIN_TOKEN", env_token)]);
    assert_eq!(server.json("GET", "/api/keys", None, &with).0, 403, "ADMIN_TOKEN wins over the saved token");
    let (st, refused) = server.json("POST", "/api/keys/admin", Some(json!({ "generate": true })), &[("X-Admin-Token", env_token)]);
    assert_eq!(st, 400);
    assert!(refused["error"].as_str().unwrap().contains("environment"), "{refused}");
    server.stop();

    // A saved token lets Wardian listen where other machines can reach it, and is kept there.
    let (mut server, port) = TestServer::spawn(&apps, &data, "0.0.0.0:0", &[]);
    match port.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(addr) => server.addr = addr.replace("0.0.0.0", "127.0.0.1"),
        Err(_) => panic!("Wardian did not start on 0.0.0.0 with a saved token:\n{}", server.stop()),
    }
    let (st, kept) = server.json("POST", "/api/keys/admin/remove", Some(json!({})), &with);
    assert_eq!(st, 400, "{kept}");
    assert!(kept["error"].as_str().unwrap().contains("other machines can reach"), "{kept}");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// A key owner whose one key comes from the environment, with nothing saved.
struct EnvironmentKey;

impl crate::usecases::keys::KeyOwner for EnvironmentKey {
    fn entries(&self) -> Vec<crate::usecases::keys::KeyEntry> {
        vec![crate::usecases::keys::KeyEntry { id: "splunk", name: "Splunk account", from: Some("environment"), detail: "token for https://splunk:8089".into(), error: None }]
    }
    fn retest(&self, id: &str) -> Option<Result<(), String>> { (id == "splunk").then_some(Ok(())) }
    fn forget(&self, id: &str) -> Option<Result<(), String>> { (id == "splunk").then_some(Ok(())) }
}

/// ADR-2610081500 #2: a key set only in the environment cannot be removed in Settings, and asking
/// to remove it keeps its last test.
#[test]
fn keys_an_environment_key_is_not_removed_and_keeps_its_test() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-env-remove");
    fs::create_dir_all(&base).unwrap();
    let (secrets, checks) = key_stores(&base);
    checks.record::<()>("splunk", &Ok(()));
    let admin = Arc::new(AdminGate::load(Arc::clone(&secrets), &base, None, false).unwrap());
    let keyring = Keyring::new(vec![Arc::new(EnvironmentKey)], admin, Arc::clone(&checks), secrets);
    let e = keyring.remove("splunk").unwrap_err();
    assert!(e.contains("environment"), "{e}");
    assert_eq!(checks.last("splunk")["ok"], true, "the key is still in use, so its test is kept");
    let splunk = keyring.list()["keys"][0].clone();
    assert_eq!((splunk["from"].as_str(), splunk["check"]["ok"].as_bool()), (Some("environment"), Some(true)), "{splunk}");
    let _ = fs::remove_dir_all(base);
}

/// On an address other machines can reach, a saved admin token cannot be removed, so the list
/// does not offer to remove it.
#[test]
fn keys_a_public_admin_token_is_not_shown_as_removable() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-public-admin");
    fs::create_dir_all(&base).unwrap();
    let (secrets, checks) = key_stores(&base);
    let admin = Arc::new(AdminGate::load(Arc::clone(&secrets), &base, None, true).unwrap());
    let keyring = Keyring::new(Vec::new(), admin, checks, secrets);
    keyring.set_admin_token(&serde_json::json!({ "token": "a".repeat(24) })).unwrap();
    let list = keyring.list();
    let entry = list["keys"].as_array().unwrap().iter().find(|k| k["id"] == "admin").unwrap().clone();
    assert_eq!(entry["from"], "settings", "{entry}");
    assert!(keyring.remove("admin").is_err(), "a public Wardian keeps its admin token");
    assert_eq!(entry["removable"], false, "the list offers a remove that always fails: {entry}");
    let _ = fs::remove_dir_all(base);
}

/// A key owner whose saved key is removed and then a later step fails, as Drive's does when it
/// cannot switch to the local apps.
struct FailsAfterRemoving;

impl crate::usecases::keys::KeyOwner for FailsAfterRemoving {
    fn entries(&self) -> Vec<crate::usecases::keys::KeyEntry> {
        vec![crate::usecases::keys::KeyEntry { id: "drive", name: "Google service account", from: None, detail: String::new(), error: None }]
    }
    fn retest(&self, id: &str) -> Option<Result<(), String>> { (id == "drive").then_some(Ok(())) }
    fn forget(&self, id: &str) -> Option<Result<(), String>> {
        (id == "drive").then(|| Err("switching to the local apps failed".into()))
    }
}

/// The check record describes the key that is there now: when an owner's forget fails after it
/// removed the saved key, the last test of that key goes too.
#[test]
fn keys_a_failed_forget_still_drops_the_removed_keys_test() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-forget-fails");
    fs::create_dir_all(&base).unwrap();
    let (secrets, checks) = key_stores(&base);
    checks.record::<()>("drive", &Ok(()));
    let admin = Arc::new(AdminGate::load(Arc::clone(&secrets), &base, None, false).unwrap());
    let keyring = Keyring::new(vec![Arc::new(FailsAfterRemoving)], admin, Arc::clone(&checks), secrets);
    let e = keyring.remove("drive").unwrap_err();
    assert!(e.contains("local apps"), "{e}");
    assert_eq!(checks.last("drive"), Value::Null, "the key is gone, so its test is gone");
    assert_eq!(keyring.list()["keys"][0]["check"], Value::Null);
    let _ = fs::remove_dir_all(base);
}

/// A key owner whose test pauses, after it starts, until the key is removed or 300 ms pass; then it
/// records a pass.
struct PausingTest {
    checks: Arc<crate::usecases::keys::KeyChecks>,
    started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
    forgot: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    forgetting: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
}

impl crate::usecases::keys::KeyOwner for PausingTest {
    fn entries(&self) -> Vec<crate::usecases::keys::KeyEntry> {
        vec![crate::usecases::keys::KeyEntry { id: "splunk", name: "Splunk account", from: Some("settings"), detail: String::new(), error: None }]
    }
    fn retest(&self, id: &str) -> Option<Result<(), String>> {
        (id == "splunk").then(|| {
            let _ = self.started.lock().unwrap().send(());
            let _ = self.forgot.lock().unwrap().recv_timeout(std::time::Duration::from_millis(300));
            let out = Ok(());
            self.checks.record("splunk", &out);
            out
        })
    }
    fn forget(&self, id: &str) -> Option<Result<(), String>> {
        (id == "splunk").then(|| {
            let _ = self.forgetting.lock().unwrap().send(());
            Ok(())
        })
    }
}

/// A test that is still running when its key is removed leaves no last test, now or after a restart.
#[test]
fn keys_a_test_running_when_its_key_is_removed_leaves_no_test() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, KeyChecks, Keyring};
    let base = tmp("keys-test-remove-race");
    fs::create_dir_all(&base).unwrap();
    let (secrets, checks) = key_stores(&base);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (forgot_tx, forgot_rx) = std::sync::mpsc::channel();
    let owner = Arc::new(PausingTest {
        checks: Arc::clone(&checks),
        started: std::sync::Mutex::new(started_tx),
        forgot: std::sync::Mutex::new(forgot_rx),
        forgetting: std::sync::Mutex::new(forgot_tx),
    });
    let admin = Arc::new(AdminGate::load(Arc::clone(&secrets), &base, None, false).unwrap());
    let keyring = Arc::new(Keyring::new(vec![owner], admin, Arc::clone(&checks), secrets));
    let test = {
        let keyring = Arc::clone(&keyring);
        std::thread::spawn(move || keyring.test("splunk").unwrap())
    };
    started_rx.recv().unwrap();
    let remove = {
        let keyring = Arc::clone(&keyring);
        std::thread::spawn(move || keyring.remove("splunk").unwrap())
    };
    test.join().unwrap();
    remove.join().unwrap();
    assert_eq!(checks.last("splunk"), Value::Null, "the key is gone, so its test is gone");
    assert_eq!(KeyChecks::new(Arc::new(LocalDisk), &base).last("splunk"), Value::Null, "and stays gone after a restart");
    let _ = fs::remove_dir_all(base);
}

/// A key-checks.json that cannot be read is moved aside, not written over by the next test.
#[test]
fn keys_an_unreadable_record_of_tests_is_kept() {
    use crate::usecases::keys::KeyChecks;
    let base = tmp("keys-checks-unreadable");
    fs::create_dir_all(&base).unwrap();
    let file = base.join("key-checks.json");
    fs::write(&file, b"{\"splunk\": {\"ok\": true, ").unwrap();
    let checks = KeyChecks::new(Arc::new(LocalDisk), &base);
    checks.record::<()>("aws", &Ok(()));
    assert_eq!(fs::read(base.join("key-checks.json.unreadable")).unwrap(), b"{\"splunk\": {\"ok\": true, ", "the old record is kept");
    assert_eq!(KeyChecks::new(Arc::new(LocalDisk), &base).last("aws")["ok"], true, "and new tests are saved");
    let _ = fs::remove_dir_all(base);
}

/// Secrets kept in memory, where writing a token that starts with "a" pauses after the write until
/// the next save is done, or 300 ms pass.
struct PausingSecrets {
    file: std::sync::Mutex<Option<Vec<u8>>>,
    wrote: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
    next_done: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl crate::ports::secrets::Secrets for PausingSecrets {
    fn read(&self, _: &Path) -> Result<Option<Vec<u8>>, String> { Ok(self.file.lock().unwrap().clone()) }
    fn write(&self, _: &Path, bytes: &[u8]) -> Result<(), String> {
        *self.file.lock().unwrap() = Some(bytes.to_vec());
        if bytes.starts_with(b"a") {
            let _ = self.wrote.lock().unwrap().send(());
            let _ = self.next_done.lock().unwrap().recv_timeout(std::time::Duration::from_millis(300));
        }
        Ok(())
    }
    fn remove(&self, _: &Path) { *self.file.lock().unwrap() = None; }
    fn new_token(&self) -> String { unreachable!() }
    fn describe(&self) -> Value { Value::Null }
}

/// Two admin tokens saved at once: the token Wardian accepts now is the one it accepts after a
/// restart.
#[test]
fn keys_two_admin_tokens_saved_at_once_leave_one_token() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-admin-race");
    fs::create_dir_all(&base).unwrap();
    let (wrote_tx, wrote_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let secrets = Arc::new(PausingSecrets {
        file: std::sync::Mutex::new(None),
        wrote: std::sync::Mutex::new(wrote_tx),
        next_done: std::sync::Mutex::new(done_rx),
    });
    let (_, checks) = key_stores(&base);
    let admin = Arc::new(AdminGate::load(secrets.clone(), &base, None, false).unwrap());
    let keyring = Arc::new(Keyring::new(Vec::new(), admin, checks, secrets.clone()));
    let (a, b) = ("a".repeat(24), "b".repeat(24));
    let first = {
        let (keyring, a) = (Arc::clone(&keyring), a.clone());
        std::thread::spawn(move || keyring.set_admin_token(&serde_json::json!({ "token": a })).unwrap())
    };
    wrote_rx.recv().unwrap();
    let second = {
        let (keyring, b) = (Arc::clone(&keyring), b.clone());
        std::thread::spawn(move || {
            keyring.set_admin_token(&serde_json::json!({ "token": b })).unwrap();
            let _ = done_tx.send(());
        })
    };
    first.join().unwrap();
    second.join().unwrap();
    let on_disk = String::from_utf8(secrets.file.lock().unwrap().clone().unwrap()).unwrap();
    assert_eq!(keyring.admin_token(), Some(on_disk), "the running token and the saved one differ");
    let _ = fs::remove_dir_all(base);
}

/// An admin token saved while it is removed: the token Wardian accepts now is the one it accepts
/// after a restart, whichever finishes last.
#[test]
fn keys_admin_token_saved_while_removed_leaves_memory_and_file_the_same() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-admin-set-forget-race");
    fs::create_dir_all(&base).unwrap();
    let (wrote_tx, wrote_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let secrets = Arc::new(PausingSecrets {
        file: std::sync::Mutex::new(None),
        wrote: std::sync::Mutex::new(wrote_tx),
        next_done: std::sync::Mutex::new(done_rx),
    });
    let (_, checks) = key_stores(&base);
    let admin = Arc::new(AdminGate::load(secrets.clone(), &base, None, false).unwrap());
    let keyring = Arc::new(Keyring::new(Vec::new(), admin, checks, secrets.clone()));
    let setter = {
        let keyring = Arc::clone(&keyring);
        std::thread::spawn(move || keyring.set_admin_token(&serde_json::json!({ "token": "a".repeat(24) })).unwrap())
    };
    wrote_rx.recv().unwrap();
    let remover = {
        let keyring = Arc::clone(&keyring);
        std::thread::spawn(move || {
            keyring.remove("admin").unwrap();
            let _ = done_tx.send(());
        })
    };
    setter.join().unwrap();
    remover.join().unwrap();
    let on_disk = secrets.file.lock().unwrap().clone().map(|b| String::from_utf8(b).unwrap());
    assert_eq!(keyring.admin_token(), on_disk, "the running token and the saved one differ");
    let _ = fs::remove_dir_all(base);
}

/// A Google that accepts any key and serves one empty folder, so Drive can be the app source.
struct DriveWithFolder;

impl crate::ports::drive::DriveConnector for DriveWithFolder {
    fn client(&self, _: &str) -> Result<Arc<dyn crate::ports::drive::DriveClient>, String> {
        Ok(Arc::new(DriveWithFolder))
    }
}

impl crate::ports::drive::DriveClient for DriveWithFolder {
    fn client_email(&self) -> String { "w@example.iam.gserviceaccount.com".into() }
    fn check(&self) -> Result<(), String> { Ok(()) }
    fn browse(&self, _: Option<&str>) -> Result<Vec<crate::domain::package::Folder>, String> { Ok(Vec::new()) }
    fn preview(&self, _: &str) -> Result<Vec<String>, String> { Ok(Vec::new()) }
    fn download(&self, _: &str) -> Result<Vec<u8>, String> { Err("nothing here".into()) }
    fn open_folder(self: Arc<Self>, _: &str, _: &str, _: std::time::Duration, _: bool) -> Result<Arc<dyn crate::ports::drive::DriveFolder>, String> {
        Ok(self)
    }
}

impl crate::ports::drive::DriveFolder for DriveWithFolder {
    fn folder_id(&self) -> String { "1AbCdEfGhIjKlMnOpQrStUvWxYz".into() }
    fn folder_name(&self) -> String { "Team apps".into() }
    fn client_email(&self) -> String { "w@example.iam.gserviceaccount.com".into() }
    fn status(&self) -> crate::domain::package::RefreshStatus { Default::default() }
    fn refresh(&self) -> Result<(), String> { Ok(()) }
    fn list_apps(&self) -> Vec<String> { Vec::new() }
    fn has(&self, _: &str, _: &str) -> bool { false }
    fn read(&self, _: &str, _: &str) -> Option<Vec<u8>> { None }
}

/// keys.md: removing the Google service account key while Drive is the app source switches the
/// source to the local apps folder, and saves that choice.
#[test]
fn keys_removing_the_drive_key_while_serving_drive_serves_local_apps() {
    use crate::ports::service::Catalog;
    use crate::usecases::keys::KeyOwner;
    let base = tmp("keys-drive-source");
    let (apps, data) = (base.join("apps"), base.join("data"));
    fs::create_dir_all(&apps).unwrap();
    let hub = hub_with_drive(&apps, &data, Arc::new(DriveWithFolder));
    hub.start(None, None);
    hub.set_key("{}").unwrap();
    Catalog::use_drive(&hub, "1AbCdEfGhIjKlMnOpQrStUvWxYz", "Team apps").unwrap();
    assert_eq!(Catalog::status(&hub)["source"], "drive");
    assert_eq!(hub.forget("drive"), Some(Ok(())));
    assert_eq!(Catalog::status(&hub)["source"], "local", "the source is local again");
    let saved: Value = serde_json::from_slice(&fs::read(data.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["source"], "local", "and the choice is saved, so a restart serves local apps too");
    let _ = fs::remove_dir_all(base);
}

/// An admin token file that is there but cannot be read stops startup: Wardian never starts with
/// no admin token because the one it was given could not be read. A folder in its place cannot
/// be read, even by root.
#[test]
fn keys_an_unreadable_admin_token_stops_startup() {
    use crate::usecases::keys::AdminGate;
    let base = tmp("keys-admin-unreadable");
    fs::create_dir_all(base.join("admin-token")).unwrap();
    let (secrets, _) = key_stores(&base);
    let e = AdminGate::load(secrets, &base, None, false).err().expect("an unreadable admin token must stop startup");
    assert!(e.contains("cannot be read"), "{e}");
    let _ = fs::remove_dir_all(base);
}

/// Secrets kept in memory whose remove leaves the file there, as an unlink refused by the disk does.
struct StuckSecrets(std::sync::Mutex<Option<Vec<u8>>>);

impl crate::ports::secrets::Secrets for StuckSecrets {
    fn read(&self, _: &Path) -> Result<Option<Vec<u8>>, String> { Ok(self.0.lock().unwrap().clone()) }
    fn write(&self, _: &Path, bytes: &[u8]) -> Result<(), String> { *self.0.lock().unwrap() = Some(bytes.to_vec()); Ok(()) }
    fn remove(&self, _: &Path) {}
    fn new_token(&self) -> String { unreachable!() }
    fn describe(&self) -> Value { Value::Null }
}

/// An admin token whose file cannot be removed: Remove says so, and the token stays in force, as it
/// will after a restart.
#[test]
fn keys_admin_token_that_cannot_be_removed_is_an_error() {
    use crate::ports::service::Keys;
    use crate::usecases::keys::{AdminGate, Keyring};
    let base = tmp("keys-admin-stuck");
    fs::create_dir_all(&base).unwrap();
    let token = "a".repeat(24);
    let secrets = Arc::new(StuckSecrets(std::sync::Mutex::new(Some(token.clone().into_bytes()))));
    let (_, checks) = key_stores(&base);
    let admin = Arc::new(AdminGate::load(secrets.clone(), &base, None, false).unwrap());
    let keyring = Keyring::new(Vec::new(), admin, checks, secrets);
    let e = keyring.remove("admin").err().expect("a token still on disk must not be reported as removed");
    assert!(e.contains("admin token"), "{e}");
    assert_eq!(keyring.admin_token(), Some(token), "the token on disk is the one in force");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081600: with no example apps on disk, Wardian gives the working folder the copies built
/// into it: every tracked example. An example new in a later version arrives in an old folder; one
/// the user removed stays removed; an app already there is not replaced.
#[test]
fn examples_built_in_fill_any_working_folder_once() {
    use crate::usecases::workspace::add_examples;
    let base = tmp("examples-built-in");
    let working = base.join("data/apps");
    let built_in = Embedded.example_apps();
    let tracked: Vec<String> = fs::read_to_string(".gitignore").unwrap().lines().filter_map(|l| l.strip_prefix("!/apps/").map(|n| n.trim_end_matches('/').to_string())).collect();
    let mut added = add_examples(&LocalDisk, None, built_in, &working).unwrap();
    added.sort();
    let mut want = tracked.clone();
    want.sort();
    assert_eq!(added, want, "every tracked example is built in and added");
    assert!(added.len() >= 16);
    for name in &tracked {
        assert!(is_app_dir(&working.join(name)), "{name} is an app");
    }
    assert!(!built_in.iter().any(|(p, _)| p.contains("/target/") || p.ends_with("Cargo.lock")), "no build output is built in");

    // Removed by the user: not added again. Changed by the user: not replaced.
    fs::remove_dir_all(working.join("life")).unwrap();
    fs::write(working.join("adder/README.md"), "mine").unwrap();
    assert!(add_examples(&LocalDisk, None, built_in, &working).unwrap().is_empty());
    assert!(!working.join("life").exists());
    assert_eq!(fs::read_to_string(working.join("adder/README.md")).unwrap(), "mine");

    // A working folder from an older Wardian, with no record: what is missing arrives.
    fs::remove_file(working.join(".examples-seen")).unwrap();
    fs::remove_dir_all(working.join("habit-tracker")).unwrap();
    let mut back = add_examples(&LocalDisk, None, built_in, &working).unwrap();
    back.sort();
    assert_eq!(back, ["habit-tracker", "life"]);
    let _ = fs::remove_dir_all(base);
}

fn is_app_dir(dir: &Path) -> bool {
    crate::domain::package::APP_MARKERS.iter().any(|m| dir.join(m).is_file())
}

/// The case that left the app list empty: a build started from a folder that is not a checkout,
/// with a data folder of its own and no apps folder named. It serves every example app.
#[test]
fn examples_a_build_started_anywhere_serves_the_examples() {
    let base = tmp("examples-anywhere");
    let (data, elsewhere) = (base.join("data"), base.join("elsewhere"));
    fs::create_dir_all(&elsewhere).unwrap();
    let cwd = elsewhere.display().to_string();
    let server = TestServer::start(Path::new("-"), &data, &[("WARDIAN_TEST_CWD", cwd.as_str())]);
    let (_, list) = server.json("GET", "/api/apps", None, &[]);
    let log = server.stop();
    assert!(list.as_array().map(Vec::len).unwrap_or(0) >= 16, "the example apps are served: {list}\n{log}");
    assert!(log.contains("apps: added"), "{log}");
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081501: every secret saved through Settings is sealed on disk: no file in the data
/// folder holds one. With another master key, a saved admin token stops the start; without it, each
/// sealed key shows as "cannot be read", and the right key opens them all again. A key saved
/// plain by an older Wardian is sealed at start.
#[test]
fn sealed_secrets_on_disk_hold_no_secret_and_need_their_master_key() {
    use serde_json::json;
    let up = fake_upstream();
    let base = tmp("sealed-disk");
    let (data, apps) = (base.join("data"), base.join("apps"));
    fs::create_dir_all(&apps).unwrap();
    fs::create_dir_all(&data).unwrap();
    // A key saved by an older Wardian, plain.
    fs::write(data.join("splunk.json"), json!({"url": up, "token": "OLD-PLAIN-SPLUNK-SECRET"}).to_string()).unwrap();
    let env = [("ANTHROPIC_BASE_URL", up.as_str()), ("WARDIAN_BEDROCK_BASE_URL", up.as_str())];
    let server = TestServer::start(&apps, &data, &env);
    let secrets = ["sk-ant-SEALED-ANTHROPIC-SECRET", "SEALED-BEDROCK-SECRET", "SEALED-ADMIN-TOKEN-SECRET-0123"];
    assert_eq!(server.json("POST", "/api/ai/key", Some(json!({"key": secrets[0]})), &[]).0, 200);
    assert_eq!(server.json("POST", "/api/ai/provider", Some(json!({"provider": "bedrock", "region": "us-east-1", "auth": "api-key", "token": secrets[1]})), &[]).0, 200);
    assert_eq!(server.json("POST", "/api/keys/admin", Some(json!({"token": secrets[2]})), &[]).0, 200);
    let with = [("X-Admin-Token", secrets[2])];
    let (_, list) = server.json("GET", "/api/keys", None, &with);
    assert_eq!(list["kept"]["sealed"], true, "{list}");
    let splunk = list["keys"].as_array().unwrap().iter().find(|k| k["id"] == "splunk").unwrap().clone();
    assert_eq!(splunk["from"], "settings", "the old plain key was read: {splunk}");
    let log = server.stop();
    assert!(log.contains("secrets: sealed with AES-256-GCM, under a master key kept in WARDIAN_MASTER_KEY"), "{log}");

    for f in fs::read_dir(&data).unwrap().flatten().filter(|f| f.path().is_file()) {
        let bytes = fs::read(f.path()).unwrap();
        for s in secrets.iter().chain(&["OLD-PLAIN-SPLUNK-SECRET"]) {
            assert!(!bytes.windows(s.len()).any(|w| w == s.as_bytes()), "{} holds {s}", f.path().display());
        }
    }
    for f in ["anthropic-key", "bedrock.json", "admin-token", "splunk.json"] {
        assert!(fs::read(data.join(f)).unwrap().starts_with(b"WARDIAN-SEALED-1\n"), "{f} is sealed");
    }

    let other = "0909090909090909090909090909090909090909090909090909090909090909";
    let (server, _) = TestServer::spawn(&apps, &data, "127.0.0.1:0", &[("WARDIAN_MASTER_KEY", other)]);
    let log = server.wait();
    assert!(log.contains("the admin token in") && log.contains("cannot be read") && log.contains("another master key"), "a token that does not open stops the start:\n{log}");

    fs::remove_file(data.join("admin-token")).unwrap();
    let server = TestServer::start(&apps, &data, &[("WARDIAN_MASTER_KEY", other)]);
    let (_, list) = server.json("GET", "/api/keys", None, &[]);
    for id in ["anthropic", "bedrock", "splunk"] {
        let k = list["keys"].as_array().unwrap().iter().find(|k| k["id"] == id).unwrap().clone();
        assert!(k["error"].as_str().is_some_and(|e| e.contains("another master key")), "{id} cannot be read: {k}");
    }
    server.stop();

    let server = TestServer::start(&apps, &data, &env);
    let (_, list) = server.json("GET", "/api/keys", None, &[]);
    let anthropic = list["keys"].as_array().unwrap().iter().find(|k| k["id"] == "anthropic").unwrap().clone();
    assert!(anthropic["error"].is_null() && anthropic["from"] == "settings", "the right key opens them again: {anthropic}");
    server.stop();
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081700: `wardian key` says where the master key is; `export` writes it to a private file
/// and keeps a file that is there; `import` restores it, and refuses one that opens no saved key.
#[test]
fn key_commands_back_up_and_restore_the_master_key() {
    use crate::adapters::primary::cli::{run, Command};
    use crate::adapters::secondary::sealed_secrets::{KeyBackup, KeyFile, KeyPlace, SealedSecrets};
    use crate::ports::secrets::Secrets;
    let base = tmp("cli-key");
    let data = base.join("data");
    fs::create_dir_all(&data).unwrap();
    let disk: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    KeyFile { fs: Arc::clone(&disk), path: base.join("master.key"), shown: "the key file".into() }.put(&[6; 32]).unwrap();
    SealedSecrets::with_key(Arc::clone(&disk), [6; 32]).write(&data.join("anthropic-key"), b"k").unwrap();
    let docs = Docs::new(Arc::new(Embedded));
    let no_exports = || -> Arc<dyn crate::ports::service::Exports> { unreachable!() };
    let no_service = || -> Box<dyn crate::ports::service_manager::Background> { unreachable!() };
    let key = |name: &'static str| {
        let (disk, base, data) = (Arc::clone(&disk), base.clone(), data.clone());
        move || -> Box<dyn crate::ports::secrets::MasterKey> {
            let place: Box<dyn KeyPlace> = Box::new(KeyFile { fs: Arc::clone(&disk), path: base.join(name), shown: name.into() });
            Box::new(KeyBackup { fs: Arc::clone(&disk), data_dir: data.clone(), place: Some(place) })
        }
    };
    let exit = |args: &[&str], name: &'static str| match run(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>(), &scaffold(), &docs, &base, &no_exports, &key(name), &no_service) {
        Command::Exit(code) => code,
        Command::Serve { .. } => panic!("key does not serve"),
    };
    assert_eq!(exit(&["key"], "master.key"), 0);
    let backup = base.join("backup.key").display().to_string();
    assert_eq!(exit(&["key", "export", &backup], "master.key"), 0);
    assert_eq!(fs::read_to_string(&backup).unwrap().trim(), "06".repeat(32));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600, "the backup is private");
    }
    assert_eq!(exit(&["key", "export", &backup], "master.key"), 1, "an existing file is kept");
    assert_eq!(exit(&["key", "export", &backup, "--force"], "master.key"), 0);
    assert_eq!(exit(&["key", "import", &backup], "restored.key"), 0, "restored into a new place");
    assert_eq!(fs::read_to_string(base.join("restored.key")).unwrap(), "06".repeat(32));
    fs::write(base.join("wrong.txt"), "07".repeat(32)).unwrap();
    let wrong = base.join("wrong.txt").display().to_string();
    assert_eq!(exit(&["key", "import", &wrong], "fresh.key"), 1, "a key that opens nothing is refused");
    assert!(!base.join("fresh.key").exists());
    assert_eq!(exit(&["key", "import"], "master.key"), 2);
    assert_eq!(exit(&["key", "--bogus"], "master.key"), 2);
    let _ = fs::remove_dir_all(base);
}

/// ADR-2610081900: each example's download on the website imports into Wardian as that app, and
/// the imported app passes `wardian check`.
#[test]
fn demos_every_website_download_imports_into_wardian() {
    use crate::usecases::import::import_zip;
    let base = tmp("demo-downloads");
    let site = Docs::new(Arc::new(Embedded)).site();
    let zips: Vec<&(String, Vec<u8>)> = site.iter().filter(|(p, _)| p.starts_with("downloads/")).collect();
    assert!(zips.len() >= 16);
    for (path, bytes) in zips {
        let app = path.trim_start_matches("downloads/").strip_suffix(".wardian").unwrap_or_else(|| panic!("{path} is a .wardian file"));
        let apps = base.join(app);
        import_zip(&LocalDisk, bytes, &format!("{app}.wardian"), &apps, false, &|_| {}).unwrap_or_else(|e| panic!("{app}: {e}"));
        let (ok, report) = checker().check_dir(&apps.join(app), app);
        assert!(ok, "{app} imported and checked: {report}");
    }
    let _ = fs::remove_dir_all(base);
}

// ---------- wardian start, stop, status (ADR-2610081800) ----------

/// The service manager and processes, simulated: launchd or systemd load and unload one job, a
/// spawn makes a live process, and whatever is set to answer once the service runs answers then.
mod service_fake {
    use crate::ports::service_manager::{Answer, Ran, System};
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Default)]
    pub struct World {
        pub calls: Vec<String>,
        pub answers: HashMap<String, Answer>,
        /// The launchd job is loaded, or the systemd unit active.
        pub loaded: bool,
        pub enabled: bool,
        /// `systemctl --user` works.
        pub systemd: bool,
        /// Live Wardian processes, and live processes that are something else.
        pub pids: HashSet<u32>,
        pub others: HashSet<u32>,
        /// What answers once the service runs; None: it never answers.
        pub starts_at: Option<(String, Answer)>,
        pub sleeps: u32,
    }

    impl World {
        fn go(&mut self) {
            if let Some((at, a)) = self.starts_at.clone() {
                self.answers.insert(at, a);
            }
        }
        fn halt(&mut self) {
            if let Some((at, _)) = &self.starts_at {
                self.answers.remove(at);
            }
        }
    }

    pub struct Fake(pub Mutex<World>);

    impl Fake {
        pub fn calls(&self) -> Vec<String> {
            self.0.lock().unwrap().calls.clone()
        }
    }

    impl System for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Option<Ran> {
            let mut w = self.0.lock().unwrap();
            w.calls.push(format!("{program} {}", args.join(" ")));
            let ok = |b: bool| Some(Ran { code: if b { 0 } else { 1 }, out: String::new() });
            match (program, args) {
                ("id", ["-u"]) => Some(Ran { code: 0, out: "501\n".into() }),
                ("launchctl", ["print", _]) => ok(w.loaded),
                ("launchctl", ["bootstrap", _, _]) => {
                    w.loaded = true;
                    w.go();
                    ok(true)
                }
                ("launchctl", ["bootout", _]) => {
                    w.loaded = false;
                    w.halt();
                    ok(true)
                }
                ("systemctl", ["--user", "show-environment"]) => ok(w.systemd),
                ("systemctl", ["--user", "is-active", ..]) => ok(w.loaded),
                ("systemctl", ["--user", "is-enabled", ..]) => ok(w.enabled),
                ("systemctl", ["--user", "restart", _]) => {
                    w.loaded = true;
                    w.go();
                    ok(true)
                }
                ("systemctl", ["--user", "stop", _]) => {
                    w.loaded = false;
                    w.halt();
                    ok(true)
                }
                ("systemctl", ["--user", "enable", ..]) => {
                    w.enabled = true;
                    ok(true)
                }
                ("systemctl", ["--user", "disable", ..]) => {
                    w.enabled = false;
                    ok(true)
                }
                ("systemctl", _) => ok(true),
                ("ps", ["-p", pid, "-o", "comm="]) => {
                    let pid: u32 = pid.parse().ok()?;
                    if w.pids.contains(&pid) {
                        Some(Ran { code: 0, out: "/opt/bin/wardian\n".into() })
                    } else if w.others.contains(&pid) {
                        Some(Ran { code: 0, out: "/usr/bin/vim\n".into() })
                    } else {
                        ok(false)
                    }
                }
                ("kill", [_, pid]) => {
                    let pid: u32 = pid.parse().ok()?;
                    let had = w.pids.remove(&pid) || w.others.remove(&pid);
                    w.halt();
                    ok(had)
                }
                _ => None,
            }
        }
        fn spawn(&self, program: &Path, env: &[(String, String)], dir: &Path, log: &Path) -> Result<u32, String> {
            let mut w = self.0.lock().unwrap();
            let vars: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            w.calls.push(format!("spawn {} [{}] in {} > {}", program.display(), vars.join(" "), dir.display(), log.display()));
            w.pids.insert(4242);
            w.go();
            Ok(4242)
        }
        fn wardian_at(&self, addr: &str) -> Option<Answer> {
            self.0.lock().unwrap().answers.get(addr).cloned()
        }
        fn sleep(&self, _: Duration) {
            self.0.lock().unwrap().sleeps += 1;
        }
    }
}

mod service_flows {
    use super::service_fake::{Fake, World};
    use super::tmp;
    use crate::adapters::secondary::local_disk::LocalDisk;
    use crate::ports::service_manager::{Answer, Background, How, Stopped};
    use crate::usecases::background::{Service, Setup};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    const VERSION: &str = "9.9.9";

    struct Rig {
        fake: Arc<Fake>,
        svc: Service,
        base: PathBuf,
        home: PathBuf,
        data: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn ours(data: &Path) -> Answer {
        Answer { version: Some(VERSION.into()), local_root: data.join("apps").display().to_string() }
    }

    fn setup(os: &'static str, home: &Path, data: &Path) -> Setup {
        Setup {
            os,
            home: home.to_path_buf(),
            config_home: home.join(".config"),
            label: "studio.wardian".into(),
            program: PathBuf::from("/opt/my bin/wardian"),
            data_dir: data.to_path_buf(),
            data_dir_as_found: data.to_path_buf(),
            addr: "127.0.0.1:8000".into(),
            addr_set: false,
            last_port: 8002,
            version: VERSION.into(),
            env: vec![("HOME".into(), home.display().to_string())],
        }
    }

    /// A service on `os`, its data folder under a folder with a space in its name, the usual port
    /// 8000 (ADDR not set, so up to 8002), nothing running.
    fn rig(tag: &str, os: &'static str, world: impl FnOnce(&mut World, &Path)) -> Rig {
        let base = tmp(&format!("service-{tag}"));
        let home = base.join("home");
        let data = home.join("Library").join("Application Support").join("Wardian");
        let mut w = World { starts_at: Some(("127.0.0.1:8000".into(), ours(&data))), ..World::default() };
        world(&mut w, &data);
        let fake = Arc::new(Fake(Mutex::new(w)));
        let svc = Service::new(fake.clone(), Arc::new(LocalDisk), setup(os, &home, &data));
        Rig { fake, svc, base, home, data }
    }

    impl Rig {
        fn agent(&self) -> PathBuf {
            self.home.join("Library/LaunchAgents/studio.wardian.plist")
        }
        fn now_only(&self) -> PathBuf {
            self.home.join(".config/wardian/studio.wardian.plist")
        }
    }

    #[test]
    fn service_launchd_start_waits_for_wardian_then_stop_unloads_it() {
        let r = rig("launchd", "macos", |_, _| {});
        let s = r.svc.start(false).expect("start");
        assert_eq!((s.how, s.already, s.at_login, s.url.as_str()), (How::Launchd, false, false, "http://127.0.0.1:8000"));
        // Not at login: the file is kept outside LaunchAgents, which launchd loads at login.
        assert_eq!(s.file.as_deref(), Some(r.now_only().as_path()));
        assert!(!r.agent().exists());
        let text = std::fs::read_to_string(r.now_only()).unwrap();
        assert!(text.contains(&format!("<key>DATA_DIR</key><string>{}</string>", r.data.display())), "{text}");
        assert!(text.contains("<string>/opt/my bin/wardian</string>"), "{text}");
        assert!(text.contains(&format!("<key>StandardOutPath</key><string>{}</string>", r.data.join("wardian.log").display())), "{text}");
        assert!(r.fake.calls().contains(&format!("launchctl bootstrap gui/501 {}", r.now_only().display())), "{:?}", r.fake.calls());

        let st = r.svc.status();
        assert!(st.running());
        assert_eq!((st.how, st.at_login, st.version.as_deref()), (Some(How::Launchd), false, Some(VERSION)));

        // --at-login on the running service: set, without starting it again.
        let again = r.svc.start(true).expect("start --at-login");
        assert!(again.already && again.at_login && again.how == How::Launchd, "{again:?}");
        assert!(r.agent().exists() && !r.now_only().exists());
        assert!(std::fs::read_to_string(r.agent()).unwrap().contains("<key>RunAtLoad</key><true/>"));
        assert_eq!(r.fake.calls().iter().filter(|c| c.starts_with("launchctl bootstrap")).count(), 1);

        match r.svc.stop().expect("stop") {
            Stopped::Stopped(How::Launchd, removed) => assert_eq!(removed, vec![r.agent()]),
            other => panic!("{other:?}"),
        }
        assert!(r.fake.calls().contains(&"launchctl bootout gui/501/studio.wardian".to_string()));
        assert!(!r.agent().exists());
        let st = r.svc.status();
        assert!(!st.running() && st.how.is_none() && !st.at_login);
    }

    #[test]
    fn service_start_without_the_flag_keeps_start_at_login() {
        let r = rig("keep", "macos", |_, _| {});
        std::fs::create_dir_all(r.agent().parent().unwrap()).unwrap();
        std::fs::write(r.agent(), "old").unwrap();
        let s = r.svc.start(false).unwrap();
        assert!(s.at_login);
        assert!(std::fs::read_to_string(r.agent()).unwrap().contains("<key>RunAtLoad</key><true/>"));
    }

    #[test]
    fn service_start_opens_the_same_wardian_already_running() {
        let r = rig("same", "macos", |w, data| {
            w.answers.insert("127.0.0.1:8000".into(), ours(data));
        });
        let s = r.svc.start(false).unwrap();
        assert!(s.already, "{s:?}");
        // Not a service: a Wardian started in a terminal.
        assert_eq!(s.how, How::Terminal);
        assert!(!r.fake.calls().iter().any(|c| c.starts_with("launchctl bootstrap")), "{:?}", r.fake.calls());
        assert!(!r.now_only().exists() && !r.agent().exists());
        // stop leaves it alone and says where it is.
        match r.svc.stop().unwrap() {
            Stopped::NotRunning { removed, terminal } => {
                assert!(removed.is_empty());
                assert_eq!(terminal.as_deref(), Some("http://127.0.0.1:8000"));
            }
            other => panic!("{other:?}"),
        }
        assert!(!r.fake.calls().iter().any(|c| c.contains("bootout")));
    }

    #[test]
    fn service_start_names_another_wardian_and_finds_its_own_on_the_next_port() {
        let r = rig("other", "macos", |w, data| {
            w.answers.insert("127.0.0.1:8000".into(), Answer { version: Some("0.1.0".into()), local_root: "/srv/apps".into() });
            w.starts_at = Some(("127.0.0.1:8001".into(), ours(data)));
        });
        let s = r.svc.start(false).unwrap();
        assert_eq!(s.url, "http://127.0.0.1:8001");
        let note = s.other.unwrap();
        assert!(note.contains("Wardian 0.1.0 serving /srv/apps") && note.contains("8000"), "{note}");
        assert!(r.svc.status().other.is_some());
    }

    #[test]
    fn service_start_says_so_when_wardian_never_answers() {
        let r = rig("silent", "macos", |w, _| {
            w.starts_at = None;
            // A job left loaded by an older start is booted out first.
            w.loaded = true;
        });
        let e = r.svc.start(false).unwrap_err();
        assert!(e.contains("did not answer within 15 s") && e.contains("wardian.log"), "{e}");
        assert_eq!(r.fake.0.lock().unwrap().sleeps, 60);
        let calls = r.fake.calls();
        let out = calls.iter().position(|c| c.starts_with("launchctl bootout")).expect("bootout");
        let boot = calls.iter().position(|c| c.starts_with("launchctl bootstrap")).expect("bootstrap");
        assert!(out < boot, "{calls:?}");
        // Loaded but silent: status says it is there and not running.
        let st = r.svc.status();
        assert_eq!((st.how, st.running()), (Some(How::Launchd), false));
    }

    #[test]
    fn service_stop_when_nothing_runs() {
        let r = rig("idle", "macos", |_, _| {});
        match r.svc.stop().unwrap() {
            Stopped::NotRunning { removed, terminal } => assert!(removed.is_empty() && terminal.is_none()),
            other => panic!("{other:?}"),
        }
        assert!(!r.fake.calls().iter().any(|c| c.contains("bootout")));
    }

    #[test]
    fn service_systemd_start_enables_at_login_and_stop_disables_it() {
        let r = rig("systemd", "linux", |w, _| w.systemd = true);
        let s = r.svc.start(true).unwrap();
        assert_eq!((s.how, s.at_login), (How::Systemd, true));
        let unit = r.home.join(".config/systemd/user/wardian.service");
        let text = std::fs::read_to_string(&unit).unwrap();
        assert!(text.contains("ExecStart=\"/opt/my bin/wardian\"") && text.contains("Restart=on-failure"), "{text}");
        let calls = r.fake.calls();
        for c in ["systemctl --user daemon-reload", "systemctl --user restart wardian.service", "systemctl --user enable --quiet wardian.service"] {
            assert!(calls.contains(&c.to_string()), "{c}: {calls:?}");
        }
        assert!(r.svc.status().at_login);
        match r.svc.stop().unwrap() {
            Stopped::Stopped(How::Systemd, removed) => assert_eq!(removed, vec![unit.clone()]),
            other => panic!("{other:?}"),
        }
        let calls = r.fake.calls();
        assert!(calls.contains(&"systemctl --user stop wardian.service".to_string()) && calls.contains(&"systemctl --user disable --quiet wardian.service".to_string()), "{calls:?}");
        assert!(!unit.exists());
        assert!(!r.svc.status().running());
    }

    #[test]
    fn service_without_a_service_manager_runs_plain_with_a_pid_file() {
        let r = rig("plain", "linux", |_, _| {});
        let s = r.svc.start(true).unwrap();
        assert_eq!((s.how, s.at_login), (How::Plain, false));
        let pid_file = r.data.join("wardian.pid");
        assert_eq!(std::fs::read_to_string(&pid_file).unwrap(), "4242\n");
        let spawn = r.fake.calls().into_iter().find(|c| c.starts_with("spawn")).unwrap();
        assert!(spawn.contains(&format!("DATA_DIR={}", r.data.display())) && spawn.contains("WARDIAN_NO_OPEN=1"), "{spawn}");
        assert!(spawn.ends_with(&format!("> {}", r.data.join("wardian.log").display())), "{spawn}");
        let st = r.svc.status();
        assert_eq!((st.how, st.running()), (Some(How::Plain), true));
        match r.svc.stop().unwrap() {
            Stopped::Stopped(How::Plain, removed) => assert_eq!(removed, vec![pid_file.clone()]),
            other => panic!("{other:?}"),
        }
        assert!(r.fake.calls().contains(&"kill -TERM 4242".to_string()));
        assert!(!pid_file.exists() && !r.svc.status().running());
    }

    #[test]
    fn service_a_stale_pid_file_is_never_used_to_end_another_process() {
        let r = rig("stale", "linux", |w, _| {
            w.others.insert(999);
        });
        let pid_file = r.data.join("wardian.pid");
        std::fs::create_dir_all(&r.data).unwrap();
        std::fs::write(&pid_file, "999\n").unwrap();
        assert!(!r.svc.status().running());
        match r.svc.stop().unwrap() {
            Stopped::NotRunning { removed, .. } => assert_eq!(removed, vec![pid_file.clone()]),
            other => panic!("{other:?}"),
        }
        assert!(!r.fake.calls().iter().any(|c| c.starts_with("kill")), "{:?}", r.fake.calls());
        assert!(r.fake.0.lock().unwrap().others.contains(&999));
        // A start over a stale file starts Wardian and replaces it.
        std::fs::write(&pid_file, "999\n").unwrap();
        r.svc.start(false).unwrap();
        assert_eq!(std::fs::read_to_string(&pid_file).unwrap(), "4242\n");
        assert!(!r.fake.calls().iter().any(|c| c.starts_with("kill")));
    }

    #[test]
    fn service_status_exits_0_when_running_and_3_when_not() {
        use crate::adapters::primary::cli::{self, Command};
        use crate::ports::service::Exports;
        let code = |r: &Rig| {
            let service = || -> Box<dyn Background> { Box::new(Service::new(r.fake.clone(), Arc::new(LocalDisk), setup("macos", &r.home, &r.data))) };
            let no_export = || -> Arc<dyn Exports> { unreachable!() };
            let no_key = || -> Box<dyn crate::ports::secrets::MasterKey> { unreachable!() };
            let docs = crate::usecases::docs::Docs::new(Arc::new(crate::adapters::secondary::embedded_assets::Embedded));
            match cli::run(&["status".to_string()], &super::scaffold(), &docs, &r.data, &no_export, &no_key, &service) {
                Command::Exit(c) => c,
                Command::Serve { .. } => panic!("status served"),
            }
        };
        let r = rig("codes", "macos", |_, _| {});
        assert_eq!(code(&r), 3);
        r.svc.start(false).unwrap();
        assert_eq!(code(&r), 0);
        r.svc.stop().unwrap();
        assert_eq!(code(&r), 3);
        // A misspelled argument is a usage error.
        let service = || -> Box<dyn Background> { unreachable!() };
        let no_export = || -> Arc<dyn Exports> { unreachable!() };
        let no_key = || -> Box<dyn crate::ports::secrets::MasterKey> { unreachable!() };
        let docs = crate::usecases::docs::Docs::new(Arc::new(crate::adapters::secondary::embedded_assets::Embedded));
        assert!(matches!(cli::run(&["start".into(), "--at-logon".into()], &super::scaffold(), &docs, &r.data, &no_export, &no_key, &service), Command::Exit(2)));
    }
}
