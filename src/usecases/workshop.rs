//! Make an app's workshop: the tools Claude uses, its prompt, and `wardian check`, all on a
//! package held in memory (ADR-2610111500). Wardian's own Make an app runs them (studio.rs, which
//! then saves to disk), and so does the hosted builder, through `wasm/workshop`. One copy of the
//! rules for both.

use crate::domain::check::{check_package, format_report, Report};
use crate::domain::components::{files_for, page_tags, wire_suite, GUIDE};
use crate::domain::package::safe_rel;
use crate::domain::studio::{free_name, size_text, EMPTY_WASM};
use crate::ports::assets::Assets;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// A package: path → bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

const MAX_FILE_CHARS: usize = 512 * 1024;
const MAX_FILES: usize = 300;

/// What one tool call did: the text for Claude, whether it is an error, the line to show the
/// user, and, for a finish whose check passed, the package's name and the summary to keep.
#[derive(Debug, Default)]
pub struct Outcome {
    pub text: String,
    pub is_error: bool,
    pub shown: Option<String>,
    /// Read by the hosted builder (wasm/workshop); Wardian's own finish saves to disk instead.
    #[allow(dead_code)]
    pub saved: Option<Saved>,
}

#[derive(Debug, PartialEq)]
pub struct Saved {
    pub name: String,
    pub summary: String,
}

fn said(text: String, is_error: bool, shown: Option<String>) -> Outcome {
    Outcome { text, is_error, shown, saved: None }
}

const INTRO: &str = r#"You build apps for Wardian, a program that runs small web apps in a sandbox. The person you talk to describes an app; you write its files. Many users are not programmers: keep your chat replies short, friendly and free of jargon.

You work on the files of ONE package through tools: list_files, read_file, write_file, delete_file, add_components, check and finish. Nothing is saved until finish succeeds, and finish runs `wardian check` first. Each finish saves a new version; the old one goes to the trash, so the user can undo.

Hard limits:
- You cannot compile Rust or WebAssembly, run npm, or write binary files. Write the app in plain JavaScript, HTML and CSS, with no build step.
- Use no external URLs: no CDN scripts, no remote fonts or images, no fetch to other sites. Put everything in the package. Draw graphics with SVG or canvas.
- `claude:sample` (AI inside the app) works in Wardian only when Claude is set up in Settings (Anthropic API or Amazon Bedrock), and only after the user allows it. `ctx.cap('sample')` can resolve to null: build the app so it still works without AI.

Choose the kind:
- **page** (an app with one job, such as a single calculator or viewer): app.json with "format": 1, "title", "description" and "page": "index.html", plus index.html and its .js/.css files. Do NOT write app.wasm: Wardian adds an empty one, which marks the folder as an app. The page runs sandboxed on an opaque origin: localStorage, sessionStorage, IndexedDB and cookies throw an error there. Do not use them. To keep data, offer download and upload of a file.
- **suite**: whenever the app has more than one job (for example a form, a result shown two ways, and an export), or the user asks for separate parts. Give each job its own part: the inputs, each view of the result, each export. Put inputs in the "aside" slot and results in "main", each in its own card, so viewers can arrange them. Keep every part's app.js under about 250 lines; split a part that grows past that. Put code that several parts need in a shared file listed in suite.json "scripts" instead of copying it. Follow section 6 of the spec exactly. Each app's contract appears twice, in suite.json and in Kernel.register, and the two must match or the kernel refuses to start the app. Frames have no network. Use ctx.store (capability "storage") to keep data, and ctx.cap('downloads') (capability "claude:downloads") to save files.

Use the Wardian component library. Every Wardian app does, so they all look and work alike:
- Call add_components first, with what the app needs (always at least button, field and card). It copies the files into ui/ and returns how to use each one. For a suite it lists them in suite.json; for a page app it returns the <link> and <script> tags to put in the page's <head>.
- Use the library's classes (w-button, w-input, w-card, w-table …) for every control. Do not write your own button, input or card styles. Take colours, spacing and fonts from its tokens (var(--w-fg), var(--w-border) …) in your own CSS.
- A page app also gets "arrange": mark each part of the page with data-panel, so each viewer can reorder, move and hide the parts. A suite gets Arrange from Wardian without this.

Make it good:
- A clean, modern, calm design. Responsive to narrow screens. Light and dark themes through prefers-color-scheme. Labels on every input. Realistic sample data, never lorem ipsum.
- Handle bad input without crashing; show a short message instead.
- Plain readable code with a comment at the top of each file that says what it does.

How to work:
- New app: say in one or two sentences what you will build. Write all files. Call check and fix every error. Then call finish with a short lowercase name (letters, digits, '-') and a one-sentence summary for the user.
- Change to an existing app: list_files, read what you need, change only what the request needs, check, finish.
- A message that starts with "[browser test]" comes from Wardian, which ran your app in a real browser. Fix what it reports, then finish again.
- After finish, reply in one or two sentences: what the app does and how to use it.

The package format specification follows, then a complete working example suite."#;

pub fn tools() -> Value {
    let path = json!({ "type": "string", "description": "Path inside the package, like \"index.html\" or \"apps/chart/app.js\". Letters, digits, '-', '_', '.' and '/'." });
    json!([
        { "name": "list_files", "description": "List the package's files with their sizes.",
          "input_schema": { "type": "object", "properties": {} } },
        { "name": "read_file", "description": "Read one text file from the package.",
          "input_schema": { "type": "object", "properties": { "path": path }, "required": ["path"] } },
        { "name": "write_file", "description": "Create or replace a text file. For a very large file, write the first part, then add the rest with append: true.",
          "input_schema": { "type": "object", "properties": {
              "path": path,
              "content": { "type": "string" },
              "append": { "type": "boolean", "description": "Add to the end of the file instead of replacing it." } },
            "required": ["path", "content"] } },
        { "name": "delete_file", "description": "Delete one file from the package.",
          "input_schema": { "type": "object", "properties": { "path": path }, "required": ["path"] } },
        { "name": "add_components", "description": "Copy components of the Wardian component library into ui/ (theme.css always comes too) and say how to use them. Components: button, field, card, badge, table, switch, tabs, dialog, toast, tooltip, progress, arrange, type, surface, bento, receipt.",
          "input_schema": { "type": "object", "properties": {
              "components": { "type": "array", "items": { "type": "string" } } },
            "required": ["components"] } },
        { "name": "check", "description": "Run `wardian check` on the package as it is now. Returns errors and warnings.",
          "input_schema": { "type": "object", "properties": {} } },
        { "name": "finish", "description": "Check the package and, if it passes, save it so the user can open it. If the check fails, nothing is saved and you get the errors.",
          "input_schema": { "type": "object", "properties": {
              "name": { "type": "string", "description": "For a new app: the folder name, lowercase, like \"habit-tracker\". Ignored when changing an existing app." },
              "summary": { "type": "string", "description": "One sentence for the user: what changed." } },
            "required": ["summary"] } }
    ])
}

pub fn system(assets: &dyn Assets) -> Value {
    let mut reference = format!("<spec>\n{}\n</spec>\n\n<example-suite>\n", assets.spec_md());
    for (path, body) in assets.example_suite() {
        reference += &format!("<file path=\"{path}\">\n{body}\n</file>\n");
    }
    reference += "</example-suite>\n(The example's \"text\" app loads a .wasm file with ctx.asset. Your apps cannot include .wasm files, so do that work in JavaScript.)";
    json!([
        { "type": "text", "text": INTRO },
        // Tools and system come first in the request, so this marker caches both.
        { "type": "text", "text": reference, "cache_control": { "type": "ephemeral" } }
    ])
}


/// `wardian check` on files in memory, with the empty app.wasm Wardian adds to a page app: the
/// same verdict as checking them written to a folder.
pub fn check(files: &Files, name: &str) -> (bool, String) {
    let mut all: Files = files.clone();
    if !all.contains_key("suite.json") && !all.contains_key("app.wasm") && all.contains_key("app.json") {
        all.insert("app.wasm".into(), EMPTY_WASM.to_vec());
    }
    let list: Vec<(String, u64)> = all.iter().map(|(p, b)| (p.clone(), b.len() as u64)).collect();
    let mut r = Report::default();
    let summary = check_package(name, &list, &|rel| all.get(rel).cloned(), &mut r);
    (r.errors.is_empty(), format_report(name, &summary, &r))
}

/// Runs one tool on the package. `app` is the app's name once saved; a new app is named from
/// finish's `name`. Finish checks and reports `saved`; storing it is the caller's.
pub fn run(assets: &dyn Assets, name: &str, input: &Value, files: &mut Files, app: Option<&str>) -> Outcome {
    let path = input["path"].as_str().unwrap_or("").trim_start_matches("./");
    let bad_path = || said(format!("\"{path}\" is not an allowed path: use letters, digits, '-', '_', '.' and '/'"), true, None);
    let (text, is_error, shown) = match name {
            "list_files" => {
                if files.is_empty() {
                    return said("The package is empty.".into(), false, None);
                }
                let list = files.iter().map(|(p, b)| format!("{p}  {}", size_text(b.len()))).collect::<Vec<_>>().join("\n");
                (list, false, Some("looked at the files".into()))
            }
            "read_file" => match files.get(path) {
                None => (format!("no file \"{path}\""), true, None),
                Some(b) => match std::str::from_utf8(b) {
                    Ok(t) => (t.to_string(), false, Some(format!("read {path}"))),
                    Err(_) => (format!("{path} is binary ({}); it cannot be shown or changed", size_text(b.len())), false, None),
                },
            },
            "write_file" => {
                if !safe_rel(path) {
                    return bad_path();
                }
                let content = input["content"].as_str().unwrap_or("");
                if path.ends_with(".wasm") {
                    return said("You cannot write .wasm files. Do the work in JavaScript.".into(), true, None);
                }
                if !files.contains_key(path) && files.len() >= MAX_FILES {
                    return said(format!("a package made here may have at most {MAX_FILES} files"), true, None);
                }
                let append = input["append"].as_bool().unwrap_or(false);
                // Measure before changing anything: a refused write leaves the file as it was.
                let kept = if append { files.get(path).map_or(0, Vec::len) } else { 0 };
                if kept + content.len() > MAX_FILE_CHARS {
                    return said(format!("{path} would be larger than 512 KB"), true, None);
                }
                let entry = files.entry(path.to_string()).or_default();
                if !append {
                    entry.clear();
                }
                entry.extend_from_slice(content.as_bytes());
                let verb = if append { "added to" } else { "wrote" };
                (format!("{verb} {path} ({})", size_text(entry.len())), false, Some(format!("{verb} {path} ({})", size_text(entry.len()))))
            }
            "delete_file" => match files.remove(path) {
                Some(_) => (format!("deleted {path}"), false, Some(format!("deleted {path}"))),
                None => (format!("no file \"{path}\""), true, None),
            },
            "add_components" => {
                let names: Vec<String> = input["components"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                let list = match files_for(&names) {
                    Ok(l) => l,
                    Err(e) => return said(e, true, None),
                };
                let rels: Vec<String> = list.iter().map(|f| format!("ui/{f}")).collect();
                let mut out = Vec::new();
                for (f, rel) in list.iter().zip(&rels) {
                    // Keep a copy the app already has: it may have been changed on purpose.
                    if !files.contains_key(rel) {
                        files.insert(rel.clone(), assets.ui_file(f).unwrap_or("").as_bytes().to_vec());
                        out.push(format!("added {rel}"));
                    }
                }
                if let Some(text) = files.get("suite.json").and_then(|b| String::from_utf8(b.clone()).ok()) {
                    match wire_suite(&text, &rels) {
                        Ok((body, wired)) => {
                            if let Some(body) = body {
                                files.insert("suite.json".into(), body.into_bytes());
                            }
                            out.extend(wired.into_iter().map(|w| format!("suite.json {w}")));
                        }
                        Err(e) => out.push(format!("could not list them in suite.json ({e}); add them to \"styles\" and \"scripts\" yourself")),
                    }
                } else {
                    out.push(format!("Put these in the page's <head>, before your own CSS and scripts:\n{}", page_tags(&rels).join("\n")));
                }
                out.push(String::new());
                out.push(GUIDE.into());
                (out.join("\n"), false, Some(format!("added components: {}", names.join(", "))))
            }
            "check" => {
                let (ok, report) = check(files, app.unwrap_or("new-app"));
                let line = if ok { "check passed" } else { "check found problems; fixing them" };
                (report, !ok, Some(line.into()))
            }
            "finish" => {
                let saved_name = app.map(String::from).unwrap_or_else(|| free_name(input["name"].as_str().unwrap_or("my-app"), &|_| false));
                let summary = input["summary"].as_str().unwrap_or("").to_string();
                let (ok, report) = check(files, &saved_name);
                if !ok {
                    return said(format!("not saved: the check found errors.\n{report}"), true, Some("not saved yet: the check found problems; fixing them".into()));
                }
                let text = format!("Saved as \"{saved_name}\". The user can open it now.");
                return Outcome { text, is_error: false, shown: None, saved: Some(Saved { name: saved_name, summary }) };
            }
            _ => (format!("unknown tool {name}"), true, None),
        };
    said(text, is_error, shown)
}

