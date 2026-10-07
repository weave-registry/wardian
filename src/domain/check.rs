//! The rules of `wardian check`: does a package follow SPEC.md, and if not, what is wrong, in
//! words. Pure: the package is given as its file list and a reader, so the same rules judge a
//! folder, a zip and an app Claude is still writing in memory.

use super::grants::valid_channel;
use super::import_plan::{MAX_FILE_BYTES, MAX_TOTAL_BYTES};
use super::package::{safe_rel, safe_segment, FORMAT, MAX_APP_FILES, MAX_DEPTH, PAGE_GUESSES, SKIP_DIRS};
use super::suite::{tag_name, FONT_CSS};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

const KNOWN_CAPS: &[&str] = &["storage", "asset", "worker", "source", "claude:downloads", "claude:sample", "splunk"];
const APP_JSON_KEYS: &[&str] = &["$schema", "format", "title", "description", "page", "channels"];
const SUITE_KEYS: &[&str] = &["$schema", "format", "title", "description", "styles", "scripts", "header", "columns", "apps"];
const ENTRY_KEYS: &[&str] = &["name", "slot", "wrap", "dir", "scripts", "emits", "listens", "provides", "needs", "caps", "channels"];

/// What a check found. A package passes when it has no errors.
#[derive(Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    fn err(&mut self, m: impl Into<String>) {
        self.errors.push(m.into());
    }
    pub fn warn(&mut self, m: impl Into<String>) {
        self.warnings.push(m.into());
    }
}

/// The report as text: a title line, then one line per error and warning, then "ok" if it passed.
pub fn format_report(title: &str, summary: &str, r: &Report) -> String {
    let mut out = format!("{title}  ({summary})\n");
    for e in &r.errors {
        out += &format!("  error    {e}\n");
    }
    for w in &r.warnings {
        out += &format!("  warning  {w}\n");
    }
    if r.errors.is_empty() {
        out += &format!("  ok       {}\n", if r.warnings.is_empty() { "follows the spec" } else { "follows the spec, with warnings" });
    }
    out
}

/// Checks one package: `files` lists every file under it (relative path, size), and `read`
/// reads one. Returns a one-line summary of what the package is.
pub fn check_package(name: &str, files: &[(String, u64)], read: &dyn Fn(&str) -> Option<Vec<u8>>, r: &mut Report) -> String {
    if !safe_segment(name) {
        r.err(format!("app name \"{name}\" is not allowed: use letters, digits, '-', '_' or '.', not starting with '.'"));
    }

    // ---- files ----
    let mut served: HashSet<String> = HashSet::new();
    let mut total = 0u64;
    let mut skipped_dirs = 0;
    for (rel, size) in files {
        let parts: Vec<&str> = rel.split('/').collect();
        // Hidden files (.gitignore, .DS_Store) are normal and simply not served.
        if parts.iter().any(|p| p.starts_with('.')) {
            continue;
        }
        if parts.iter().any(|p| SKIP_DIRS.contains(p)) {
            // target/ is just Rust's build cache; node_modules may be a real mistake.
            if parts.contains(&"node_modules") {
                skipped_dirs += 1;
            }
            continue;
        }
        if parts.len() > MAX_DEPTH + 1 {
            r.err(format!("{rel}: nested deeper than {MAX_DEPTH} folders"));
        } else if !safe_rel(rel) {
            r.err(format!("{rel}: name not allowed, so it would not be served (use letters, digits, '-', '_', '.')"));
        } else {
            served.insert(rel.clone());
        }
        if *size > MAX_FILE_BYTES {
            r.err(format!("{rel}: larger than 64 MB"));
        }
        total += size;
    }
    if skipped_dirs > 0 {
        r.warn(format!("{skipped_dirs} file(s) in node_modules are never served; ship built files instead"));
    }
    if served.len() > MAX_APP_FILES {
        r.err(format!("{} files; a package may have at most {MAX_APP_FILES}", served.len()));
    }
    if total > MAX_TOTAL_BYTES {
        r.err("more than 256 MB in total");
    }

    // ---- app.json ----
    let mut page: Option<String> = None;
    if let Some(bytes) = read("app.json") {
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(m)) => {
                check_format(&m, "app.json", r);
                unknown_keys(&m, APP_JSON_KEYS, "app.json", r);
                check_channels(m.get("channels"), doc_format(&m), "app.json: channels", r);
                for k in ["title", "description", "page"] {
                    if m.get(k).is_some_and(|v| !v.is_string()) {
                        r.err(format!("app.json: \"{k}\" must be a string"));
                    }
                }
                if let Some(p) = m.get("page").and_then(Value::as_str) {
                    if !safe_rel(p) || !served.contains(p) {
                        r.err(format!("app.json: page \"{p}\" is not a file in the package"));
                    }
                    page = Some(p.to_string());
                }
            }
            Ok(_) => r.err("app.json must be a JSON object"),
            Err(e) => r.err(format!("app.json is not valid JSON: {e}")),
        }
    }

    // ---- kind ----
    let is_suite = served.contains("suite.json");
    let has_wasm = served.contains("app.wasm");
    if !is_suite && !has_wasm {
        r.err("no app.wasm and no suite.json at the top: this is not a package");
    }
    if has_wasm {
        match read("app.wasm") {
            Some(b) if b.len() >= 8 && b.starts_with(b"\0asm") && b[4..8] == [1, 0, 0, 0] => {}
            Some(b) if b.starts_with(b"\0asm") => r.err("app.wasm: unsupported WebAssembly binary version (need version 1)"),
            _ => r.err("app.wasm is not a WebAssembly module"),
        }
    }
    if is_suite {
        if has_wasm || page.is_some() {
            r.warn("suite.json is present, so the host runs the suite; app.wasm and the page are not shown");
        }
        let n = check_suite(&served, read, r);
        check_library(&served, None, read, r);
        return format!("suite, {n} apps, {} files, {} KB", served.len(), total.div_ceil(1024));
    }
    let page = page.or_else(|| {
        PAGE_GUESSES.iter().find(|p| served.contains(**p)).map(|p| p.to_string())
    });
    if let Some(p) = &page {
        check_library(&served, Some(p), read, r);
    }
    match page {
        Some(p) => format!("module with page {p}, {} files, {} KB", served.len(), total.div_ceil(1024)),
        None => format!("module, functions only, {} files, {} KB", served.len(), total.div_ceil(1024)),
    }
}

/// Every app uses the Wardian component library, and every page app offers Arrange (SPEC.md 6.10).
/// Only warnings: an app without them still runs. A module app needs neither, since Wardian draws
/// it with the library, and a suite gets Arrange from the kernel.
fn check_library(served: &HashSet<String>, page: Option<&str>, read: &dyn Fn(&str) -> Option<Vec<u8>>, r: &mut Report) {
    if !served.iter().any(|f| f == "theme.css" || f.ends_with("/theme.css")) {
        r.warn("does not use the Wardian component library: run `wardian add button field card <package>` and use its classes (SPEC.md 6.10)");
    }
    let Some(page) = page else { return };
    let html = read(page).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    if !html.contains("data-panel") {
        r.warn(format!("{page}: no element has data-panel, so viewers cannot arrange this page; mark its parts and run `wardian add arrange <package>`"));
    } else if !html.contains("arrange.js") {
        r.warn(format!("{page}: marks panels but does not load arrange.js; run `wardian add arrange <package>`"));
    }
}

fn check_format(m: &Map<String, Value>, file: &str, r: &mut Report) {
    match m.get("format") {
        None => {}
        Some(Value::Number(n)) if n.as_u64().is_some_and(|f| (1..=FORMAT).contains(&f)) => {}
        Some(Value::Number(n)) if n.as_u64().is_some_and(|f| f > FORMAT) => {
            r.err(format!("{file}: format {n} is newer than this Wardian supports ({FORMAT})"))
        }
        Some(v) => r.err(format!("{file}: \"format\" must be a whole number from 1, not {v}")),
    }
}

fn doc_format(m: &Map<String, Value>) -> u64 {
    m.get("format").and_then(Value::as_u64).unwrap_or(1)
}

/// "channels": {"send": [...], "receive": [...]}, which needs format 2.
fn check_channels(v: Option<&Value>, format: u64, at: &str, r: &mut Report) {
    let Some(v) = v else { return };
    let Value::Object(m) = v else {
        r.err(format!("{at}: must be an object like {{\"send\": [\"budget\"], \"receive\": []}}"));
        return;
    };
    if format < 2 {
        r.err(format!("{at}: channels need \"format\": 2, so an older Wardian says \"update\" instead of failing"));
    }
    for k in m.keys().filter(|k| !matches!(k.as_str(), "send" | "receive")) {
        r.err(format!("{at}: unknown field \"{k}\" (use send and receive)"));
    }
    for mode in ["send", "receive"] {
        for name in strings(m.get(mode), &format!("{at}.{mode}"), r) {
            if !valid_channel(name) {
                r.err(format!("{at}.{mode}: \"{name}\" is not a channel name (lowercase letters, digits, '.', '-', '_')"));
            }
        }
    }
}

fn unknown_keys(m: &Map<String, Value>, known: &[&str], at: &str, r: &mut Report) {
    for k in m.keys().filter(|k| !known.contains(&k.as_str())) {
        r.warn(format!("{at}: unknown field \"{k}\" is ignored (a typo?)"));
    }
}

fn strings<'a>(v: Option<&'a Value>, at: &str, r: &mut Report) -> Vec<&'a str> {
    match v {
        None => vec![],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| {
                let s = x.as_str();
                if s.is_none() {
                    r.err(format!("{at}: every entry must be a string"));
                }
                s
            })
            .collect(),
        Some(_) => {
            r.err(format!("{at} must be a list of strings"));
            vec![]
        }
    }
}

fn check_script(rel: &str, served: &HashSet<String>, read: &dyn Fn(&str) -> Option<Vec<u8>>, at: &str, r: &mut Report) {
    if !safe_rel(rel) || !served.contains(rel) {
        return r.err(format!("{at}: \"{rel}\" is not a file in the package"));
    }
    match read(rel).map(String::from_utf8) {
        Some(Ok(code)) if code.to_ascii_lowercase().contains("</script") => r.err(format!("{rel} contains \"</script\", which would break the frame")),
        Some(Err(_)) => r.err(format!("{rel} is not UTF-8 text")),
        _ => {}
    }
}

/// Returns how many apps the suite lists.
fn check_suite(served: &HashSet<String>, read: &dyn Fn(&str) -> Option<Vec<u8>>, r: &mut Report) -> usize {
    let s = match read("suite.json").map(|b| serde_json::from_slice::<Value>(&b)) {
        Some(Ok(Value::Object(m))) => m,
        Some(Ok(_)) => {
            r.err("suite.json must be a JSON object");
            return 0;
        }
        Some(Err(e)) => {
            r.err(format!("suite.json is not valid JSON: {e}"));
            return 0;
        }
        None => return 0,
    };
    check_format(&s, "suite.json", r);
    unknown_keys(&s, SUITE_KEYS, "suite.json", r);
    let format = doc_format(&s);

    for style in strings(s.get("styles"), "suite.json: styles", r) {
        if style.starts_with(FONT_CSS) {
            continue;
        }
        if style.starts_with("http:") || style.starts_with("https:") {
            r.err(format!("suite.json: styles: \"{style}\" — the only outside stylesheets allowed are from {FONT_CSS}"));
        } else if !safe_rel(style) || !served.contains(style) {
            r.err(format!("suite.json: styles: \"{style}\" is not a file in the package"));
        } else if read(style).is_some_and(|b| String::from_utf8_lossy(&b).to_ascii_lowercase().contains("</style")) {
            r.err(format!("{style} contains \"</style\", which would break the frame"));
        }
    }
    for rel in strings(s.get("scripts"), "suite.json: scripts", r) {
        check_script(rel, served, read, "suite.json: scripts", r);
    }
    if let Some(h) = s.get("header") {
        match h.as_str() {
            Some(rel) if safe_rel(rel) && served.contains(rel) => {}
            _ => r.err("suite.json: header must name a file in the package"),
        }
    }
    if let Some(c) = s.get("columns") {
        let ok = c.as_str().is_some_and(|c| c.chars().all(|ch| ch.is_ascii_alphanumeric() || " _(),.%-".contains(ch)));
        if !ok {
            r.warn("suite.json: columns uses characters the kernel ignores; the default layout is used");
        }
    }

    let Some(Value::Array(apps)) = s.get("apps") else {
        r.err("suite.json needs an \"apps\" list");
        return 0;
    };
    if apps.is_empty() {
        r.err("suite.json: \"apps\" is empty");
    }

    // First pass: names, and what each app provides and emits.
    let mut provides: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut names = HashSet::new();
    for (i, a) in apps.iter().enumerate() {
        let Some(name) = a.get("name").and_then(Value::as_str) else { continue };
        let at = format!("suite.json: apps[{i}] ({name})");
        let p = strings(a.get("provides"), &format!("{at}.provides"), &mut Report::default());
        provides.insert(name.to_string(), p.into_iter().map(String::from).collect());
        if let Some(Value::Object(e)) = a.get("emits") {
            emitted.extend(e.keys().cloned());
        }
        if !names.insert(name.to_string()) {
            r.err(format!("{at}: two apps are named \"{name}\""));
        }
    }

    // Second pass: every entry in full.
    for (i, a) in apps.iter().enumerate() {
        let Value::Object(a) = a else {
            r.err(format!("suite.json: apps[{i}] must be an object"));
            continue;
        };
        let name = a.get("name").and_then(Value::as_str).unwrap_or("");
        let at = format!("suite.json: apps[{i}] ({name})");
        if !safe_segment(name) {
            r.err(format!("{at}: name must use letters, digits, '-', '_' or '.'"));
            continue;
        }
        unknown_keys(a, ENTRY_KEYS, &at, r);
        check_channels(a.get("channels"), format, &format!("{at}.channels"), r);
        let slot = a.get("slot").map(|v| v.as_str().unwrap_or("?"));
        if !matches!(slot, None | Some("aside") | Some("main")) {
            r.err(format!("{at}: slot must be \"aside\", \"main\", or absent"));
        }
        if let Some(w) = a.get("wrap") {
            if w.as_str().and_then(tag_name).is_none() {
                r.err(format!("{at}: wrap must be one opening tag, like <section id=\"x\">"));
            }
            if slot.is_none() {
                r.warn(format!("{at}: wrap has no effect on an app with no slot"));
            }
        }
        let dir = match a.get("dir") {
            Some(v) => v.as_str().unwrap_or("").to_string(),
            None => format!("apps/{name}"),
        };
        if !safe_rel(&dir) {
            r.err(format!("{at}: dir \"{dir}\" is not allowed"));
            continue;
        }
        check_script(&format!("{dir}/app.js"), served, read, &at, r);
        if slot.is_some() && !served.contains(&format!("{dir}/view.html")) {
            r.warn(format!("{at}: has a slot but no {dir}/view.html, so its view is empty"));
        }
        for rel in strings(a.get("scripts"), &format!("{at}.scripts"), r) {
            check_script(rel, served, read, &format!("{at}.scripts"), r);
        }

        // The contract.
        match a.get("emits") {
            None => {}
            Some(Value::Object(e)) => {
                for (topic, rule) in e {
                    let ok = rule.as_object().is_some_and(|o| o.keys().all(|k| k == "retain") && o.get("retain").is_none_or(Value::is_boolean));
                    if !ok {
                        r.err(format!("{at}.emits.{topic}: must be {{}} or {{\"retain\": true|false}}"));
                    }
                }
            }
            Some(_) => r.err(format!("{at}.emits must be an object of topic → {{\"retain\": bool}}")),
        }
        for topic in strings(a.get("listens"), &format!("{at}.listens"), r) {
            if !emitted.contains(topic) {
                r.warn(format!("{at}: listens to \"{topic}\", which no app in the suite emits"));
            }
        }
        strings(a.get("provides"), &format!("{at}.provides"), r);
        for need in strings(a.get("needs"), &format!("{at}.needs"), r) {
            match need.split_once('.') {
                Some((app, method)) => match provides.get(app) {
                    None => r.err(format!("{at}: needs \"{need}\", but there is no app \"{app}\"")),
                    Some(p) if !p.contains(method) => r.err(format!("{at}: needs \"{need}\", but {app} does not provide \"{method}\"")),
                    _ => {}
                },
                None => r.err(format!("{at}: needs \"{need}\" must be \"app.method\"")),
            }
        }
        for cap in strings(a.get("caps"), &format!("{at}.caps"), r) {
            if !KNOWN_CAPS.contains(&cap) {
                r.warn(format!("{at}: capability \"{cap}\" is unknown to this Wardian and is never granted"));
            }
        }
    }
    apps.len()
}
