//! The Wardian UI components, and `wardian add <component...> <package>`.
//!
//! Like shadcn, the components are copied into a package rather than loaded
//! from Wardian: the package keeps its own copy, stays complete on its own,
//! and its author may change the copy. The files are built into the program
//! from static/ui/, and the gallery at /ui/ shows every one.

use serde_json::Value;
use std::{fs, path::Path};

macro_rules! ui {
    ($f:literal) => {
        ($f, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/ui/", $f)))
    };
}

/// Every file, by name. theme.css comes with every component.
const FILES: &[(&str, &str)] = &[
    ui!("theme.css"),
    ui!("button.css"),
    ui!("field.css"),
    ui!("card.css"),
    ui!("badge.css"),
    ui!("table.css"),
    ui!("switch.css"),
    ui!("tabs.css"),
    ui!("tabs.js"),
    ui!("dialog.css"),
    ui!("dialog.js"),
    ui!("toast.css"),
    ui!("toast.js"),
    ui!("tooltip.css"),
    ui!("tooltip.js"),
    ui!("progress.js"),
    ui!("arrange.js"),
];

/// The gallery page at /ui/.
pub const GALLERY: &str = include_str!("../static/ui/index.html");

/// (name, what it is, its files).
const COMPONENTS: &[(&str, &str, &[&str])] = &[
    ("button", "buttons in five variants and three sizes", &["button.css"]),
    ("field", "text inputs, selects, text areas, labels and hints", &["field.css"]),
    ("card", "a bordered box with a title, a description, content and a footer", &["card.css"]),
    ("badge", "a small label for a state or a count", &["badge.css"]),
    ("table", "a data table with a sticky header and right-aligned numbers", &["table.css"]),
    ("switch", "an on/off switch made from a checkbox", &["switch.css"]),
    ("tabs", "tabs with arrow-key movement between them", &["tabs.css", "tabs.js"]),
    ("dialog", "a modal dialog, and WardianUI.confirm()", &["dialog.css", "dialog.js"]),
    ("toast", "short messages in the corner: WardianUI.toast()", &["toast.css", "toast.js"]),
    ("tooltip", "a hint on hover and keyboard focus", &["tooltip.css", "tooltip.js"]),
    ("progress", "the <wardian-progress> bar (Wardian also provides it built in)", &["progress.js"]),
    ("arrange", "Arrange: each viewer may reorder, move and hide the panels marked data-panel", &["arrange.js"]),
];

pub fn file(name: &str) -> Option<&'static str> {
    FILES.iter().find(|(n, _)| *n == name).map(|(_, body)| *body)
}

/// What `add` did, for the message and for tests.
#[derive(Debug, Default, PartialEq)]
pub struct Added {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
    /// Lines added to suite.json, or the tags a page app must add by hand.
    pub wired: Vec<String>,
    pub page_tags: Vec<String>,
}

/// The files the named components need, theme.css first.
pub fn files_for(names: &[String]) -> Result<Vec<&'static str>, String> {
    if names.is_empty() {
        return Err("name at least one component".into());
    }
    let mut files: Vec<&str> = vec!["theme.css"];
    for n in names {
        let (_, _, fs_) = COMPONENTS
            .iter()
            .find(|(c, _, _)| c == n)
            .ok_or_else(|| format!("unknown component \"{n}\" (see wardian add --list)"))?;
        for f in *fs_ {
            if !files.contains(f) {
                files.push(f);
            }
        }
    }
    Ok(files)
}

/// Lists `rels` (ui/… files) in suite.json's styles and scripts, before the package's own files so
/// the package's styles win. Returns the new text, or None when nothing changed, and what was added.
pub fn wire_suite(text: &str, rels: &[String]) -> Result<(Option<String>, Vec<String>), String> {
    let mut s: Value = serde_json::from_str(text).map_err(|e| format!("suite.json: {e}"))?;
    let obj = s.as_object_mut().ok_or("suite.json is not an object")?;
    let mut wired = Vec::new();
    for (key, ext) in [("styles", ".css"), ("scripts", ".js")] {
        let list = obj.entry(key).or_insert_with(|| Value::Array(vec![]));
        let arr = list.as_array_mut().ok_or_else(|| format!("suite.json: {key} is not a list"))?;
        let mut at = arr.iter().rposition(|v| v.as_str().is_some_and(|s| s.starts_with("ui/"))).map_or(0, |i| i + 1);
        for r in rels.iter().filter(|r| r.ends_with(ext)) {
            if !arr.iter().any(|v| v.as_str() == Some(r)) {
                arr.insert(at, Value::String(r.clone()));
                at += 1;
                wired.push(format!("{key}: {r}"));
            }
        }
    }
    if wired.is_empty() {
        return Ok((None, wired));
    }
    Ok((Some(serde_json::to_string_pretty(&s).map_err(|e| e.to_string())? + "\n"), wired))
}

/// The tags a page app puts in its <head> for `rels`.
pub fn page_tags(rels: &[String]) -> Vec<String> {
    rels.iter()
        .map(|r| if r.ends_with(".css") { format!("<link rel=\"stylesheet\" href=\"{r}\">") } else { format!("<script src=\"{r}\"></script>") })
        .collect()
}

/// How to use each component, for Claude when it builds an app (Make an app).
pub const GUIDE: &str = r#"Classes and helpers of the Wardian component library. Colours come from theme.css tokens: var(--w-bg), --w-fg, --w-muted, --w-muted-bg, --w-border, --w-primary, --w-primary-fg, --w-destructive, --w-success, --w-radius, --w-space-1..6, --w-font, --w-font-mono. Use them for your own styles too; never hard-code a palette.
- button: <button class="w-button">Save</button>; data-variant="secondary|outline|ghost|destructive"; data-size="sm|lg|icon".
- field: <div class="w-field"><label class="w-label" for="x">Name</label><input class="w-input" id="x"><p class="w-hint">Help</p></div>; .w-input also fits <select> and <textarea>; aria-invalid="true" shows an error.
- card: <section class="w-card"><div class="w-card-header"><h2 class="w-card-title">Title</h2><p class="w-card-description">…</p></div><div class="w-card-content">…</div><div class="w-card-footer">…</div></section>
- badge: <span class="w-badge">New</span>; data-variant="secondary|outline|destructive|success".
- table: <table class="w-table"> with <thead>/<tbody>; class "num" on number cells right-aligns them.
- switch: <input type="checkbox" class="w-switch" role="switch">
- tabs: <div class="w-tabs"><div role="tablist"><button role="tab" aria-controls="p1">One</button>…</div><div role="tabpanel" id="p1">…</div>…</div> (tabs.js wires keys and ARIA).
- dialog: <dialog class="w-dialog">…</dialog> with showModal(); await WardianUI.confirm({title, text, confirm: 'Delete', destructive: true}).
- toast: WardianUI.toast('Saved', {variant: 'success'|'destructive'}).
- tooltip: data-tooltip="Hint" on any element.
- progress: <wardian-progress label="Loading"></wardian-progress>; .start(label), .update({value, max}), .done(text), .fail(text).
- arrange (page apps): mark each part <section data-panel="name" data-panel-label="Name">; put panels in containers data-arrange-column="side" / "main" inside a data-arrange-grid element, or in one parent; viewers can then reorder, move and hide them. A suite gets Arrange from Wardian without this."#;

/// Copies the components' files into `<package>/ui/` and wires them in.
pub fn add(names: &[String], pkg: &Path, force: bool) -> Result<Added, String> {
    let files = files_for(names)?;
    let suite_json = pkg.join("suite.json");
    let app_json = pkg.join("app.json");
    let is_suite = suite_json.is_file();
    if !is_suite {
        let app: Value = fs::read_to_string(&app_json)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or_else(|| format!("{} is not a package: it has no suite.json or app.json", pkg.display()))?;
        if app.get("page").and_then(Value::as_str).is_none() {
            return Err("this is a module app: Wardian draws its page, so it has nowhere to use components. Use a page app or a suite".into());
        }
    }

    let mut out = Added::default();
    let dir = pkg.join("ui");
    fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    for f in &files {
        let path = dir.join(f);
        let rel = format!("ui/{f}");
        if path.exists() && !force {
            out.skipped.push(rel);
            continue;
        }
        fs::write(&path, file(f).unwrap()).map_err(|e| format!("writing {}: {e}", path.display()))?;
        out.written.push(rel);
    }

    let rels: Vec<String> = files.iter().map(|f| format!("ui/{f}")).collect();
    if is_suite {
        let text = fs::read_to_string(&suite_json).map_err(|e| format!("reading suite.json: {e}"))?;
        let (body, wired) = wire_suite(&text, &rels)?;
        out.wired = wired;
        if let Some(body) = body {
            fs::write(&suite_json, body).map_err(|e| format!("writing suite.json: {e}"))?;
        }
    } else {
        out.page_tags = page_tags(&rels);
    }
    Ok(out)
}

/// The `wardian add` command. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let list = || {
        println!("components (copied into PACKAGE/ui/, with theme.css):");
        for (n, what, _) in COMPONENTS {
            println!("  {n:<9} {what}");
        }
        println!("\nsee them all: open /ui/ on a running Wardian");
    };
    if args.iter().any(|a| a == "--list" || a == "-l") {
        list();
        return 0;
    }
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    let rest: Vec<String> = args.iter().filter(|a| !a.starts_with('-')).cloned().collect();
    let Some((pkg, names)) = rest.split_last().filter(|(_, n)| !n.is_empty()) else {
        eprintln!("usage: wardian add [--force] COMPONENT... PACKAGE\n       wardian add --list\n\nexample: wardian add button tabs toast apps/my-suite\n");
        list();
        return 2;
    };
    let pkg = Path::new(pkg);
    match add(names, pkg, force) {
        Err(e) => {
            eprintln!("wardian add: {e}");
            1
        }
        Ok(a) => {
            for w in &a.written {
                println!("  wrote    {}/{w}", pkg.display());
            }
            for s in &a.skipped {
                println!("  kept     {}/{s} (already there; --force replaces it)", pkg.display());
            }
            for w in &a.wired {
                println!("  added to suite.json {w}");
            }
            if !a.page_tags.is_empty() {
                println!("\nadd these lines to your page's <head>:");
                for t in &a.page_tags {
                    println!("  {t}");
                }
            }
            println!();
            crate::check::run(&[pkg.display().to_string()])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("wardian-ui-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_component_file_exists() {
        for (_, _, files) in COMPONENTS {
            for f in *files {
                assert!(file(f).is_some(), "{f} is not built in");
            }
        }
    }

    #[test]
    fn add_copies_and_wires_a_suite() {
        // A bare suite, not `wardian new`: the templates already carry the library.
        let dir = tmp("suite").join("demo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("suite.json"), "{\n  \"$schema\": \"http://127.0.0.1:8000/schemas/suite.schema.json\",\n  \"format\": 1,\n  \"title\": \"Demo\",\n  \"styles\": [\"style.css\"],\n  \"apps\": [{ \"name\": \"a\" }]\n}\n").unwrap();
        fs::write(dir.join("style.css"), "").unwrap();
        fs::create_dir_all(dir.join("apps/a")).unwrap();
        fs::write(dir.join("apps/a/app.js"), "Kernel.register({ name: 'a', init() {} });\n").unwrap();
        let a = add(&names(&["button", "tabs", "toast"]), &dir, false).unwrap();
        assert!(a.written.contains(&"ui/theme.css".into()) && a.written.contains(&"ui/tabs.js".into()));
        assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), file("button.css").unwrap());
        let s: Value = serde_json::from_str(&fs::read_to_string(dir.join("suite.json")).unwrap()).unwrap();
        let styles: Vec<&str> = s["styles"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(styles, ["ui/theme.css", "ui/button.css", "ui/tabs.css", "ui/toast.css", "style.css"]);
        assert_eq!(s["scripts"], serde_json::json!(["ui/tabs.js", "ui/toast.js"]));
        // suite.json keeps its key order.
        let keys: Vec<&String> = s.as_object().unwrap().keys().collect();
        assert_eq!(keys[..3], ["$schema", "format", "title"]);
        assert!(crate::check::check_path(&dir), "the suite fails wardian check after add");

        // A second add keeps the author's copies and wires nothing twice.
        fs::write(dir.join("ui/button.css"), "/* mine */").unwrap();
        let b = add(&names(&["button", "dialog"]), &dir, false).unwrap();
        assert!(b.skipped.contains(&"ui/button.css".into()) && b.skipped.contains(&"ui/theme.css".into()));
        assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), "/* mine */");
        assert_eq!(b.wired, ["styles: ui/dialog.css", "scripts: ui/dialog.js"]);
        let s: Value = serde_json::from_str(&fs::read_to_string(dir.join("suite.json")).unwrap()).unwrap();
        assert_eq!(s["styles"].as_array().unwrap().iter().filter(|v| v == &"ui/button.css").count(), 1);
        // --force replaces it.
        add(&names(&["button"]), &dir, true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("ui/button.css")).unwrap(), file("button.css").unwrap());
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn add_to_a_page_gives_tags_and_refuses_modules_and_unknowns() {
        let base = tmp("page");
        let page = base.join("pg");
        crate::new::create("page", &page).unwrap();
        let a = add(&names(&["dialog"]), &page, false).unwrap();
        assert_eq!(a.page_tags, ["<link rel=\"stylesheet\" href=\"ui/theme.css\">", "<link rel=\"stylesheet\" href=\"ui/dialog.css\">", "<script src=\"ui/dialog.js\"></script>"]);
        assert!(a.wired.is_empty());
        let module = base.join("md");
        crate::new::create("module", &module).unwrap();
        assert!(add(&names(&["button"]), &module, false).unwrap_err().contains("module app"));
        assert!(add(&names(&["sparkles"]), &page, false).unwrap_err().contains("unknown component"));
        let _ = fs::remove_dir_all(base);
    }
}
