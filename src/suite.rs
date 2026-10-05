//! Suites: several small apps that work together on one screen.
//!
//! A suite folder holds a `suite.json` that lists its apps, their contracts
//! and where each one sits. The host kernel page (static/kernel.html) draws
//! the layout and gives each app its own sandboxed frame, built here. Apps
//! talk only through the kernel, by copied messages, and the kernel enforces
//! the contracts in suite.json, never what an app's own code claims.
//!
//! Each frame is one self-contained document: the suite's styles and shared
//! scripts inlined, the app's view inside its wrapper, the shim that plays
//! `Kernel` inside the frame, then the app's script.

use crate::source::{safe_rel, safe_segment, Source};
use serde::Deserialize;

/// The shim that plays `Kernel` inside each frame.
const SHIM_JS: &str = include_str!("../static/shim.js");

/// Fonts are the one outside resource a frame may load.
pub const FONT_CSS: &str = "https://fonts.googleapis.com/";

/// The rules a frame runs under. `sandbox` gives it a throwaway origin (no
/// cookies, storage, or access to the host page). The rest blocks every
/// network request: anything an app needs comes from the kernel. 'wasm-unsafe-eval'
/// lets an app compile WebAssembly (not JavaScript eval) from bytes it got
/// through ctx.asset.
pub const FRAME_CSP: &str = "sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads; \
    default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:; \
    style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; \
    img-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'";

#[derive(Deserialize)]
struct Suite {
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    scripts: Vec<String>,
    header: Option<String>,
    apps: Vec<SuiteApp>,
}

/// The parts of an app entry the frame needs. The contract fields (emits,
/// listens, ...) are read by the kernel page, not here.
#[derive(Deserialize)]
struct SuiteApp {
    name: String,
    /// "aside", "main", or absent for an app with no view.
    slot: Option<String>,
    /// The opening tag the view sits in, e.g. `<section id="chartSec">`.
    wrap: Option<String>,
    /// Folder holding app.js and view.html; default `apps/<name>`.
    dir: Option<String>,
    #[serde(default)]
    scripts: Vec<String>,
}

fn text(src: &Source, suite: &str, rel: &str) -> Result<String, String> {
    if !safe_rel(rel) {
        return Err(format!("path not allowed: {rel}"));
    }
    let bytes = src.read(suite, rel).ok_or_else(|| format!("missing file: {rel}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{rel} is not UTF-8 text"))
}

/// Inlines a script so `ctx.source("<stem>-src")` can read it back, as in the
/// single-file build.
fn script_tag(src: &Source, suite: &str, rel: &str) -> Result<String, String> {
    let code = text(src, suite, rel)?;
    if code.to_ascii_lowercase().contains("</script") {
        return Err(format!("{rel} contains </script"));
    }
    let stem = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".js");
    Ok(format!("<script id=\"{stem}-src\">\n{code}\n</script>\n"))
}

/// The tag name of an opening tag like `<section id="x">`, for its closing tag.
pub fn tag_name(open: &str) -> Option<&str> {
    let name = open.strip_prefix('<')?.split(|c: char| c.is_whitespace() || c == '>').next()?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())).then_some(name)
}

// display:flow-root keeps child margins inside <body>, so the height the
// shim reports to the kernel is the whole of what the frame shows.

/// Builds the document for one frame: an app, or the suite header when
/// `app` is None.
pub fn frame(src: &Source, suite: &str, app: Option<&str>) -> Result<String, String> {
    let s: Suite = serde_json::from_slice(&src.read(suite, "suite.json").ok_or("no suite.json")?)
        .map_err(|e| format!("suite.json: {e}"))?;

    let mut head = String::new();
    for style in &s.styles {
        if style.starts_with(FONT_CSS) {
            if style.contains(['"', '<', '>']) {
                return Err("bad font URL".into());
            }
            head.push_str(&format!("<link rel=\"stylesheet\" href=\"{style}\">\n"));
        } else {
            let css = text(src, suite, style)?;
            if css.to_ascii_lowercase().contains("</style") {
                return Err(format!("{style} contains </style"));
            }
            head.push_str(&format!("<style>\n{css}\n</style>\n"));
        }
    }

    let (body, scripts) = match app {
        None => {
            let rel = s.header.as_deref().ok_or("suite has no header")?;
            let shim = format!("<script>\n{SHIM_JS}\n</script>\n");
            (format!("<div data-app=\"header\">{}</div>", text(src, suite, rel)?), shim)
        }
        Some(name) => {
            let pos = s.apps.iter().position(|a| a.name == name).ok_or_else(|| format!("no app \"{name}\" in suite.json"))?;
            let a = &s.apps[pos];
            if !safe_segment(&a.name) {
                return Err(format!("bad app name: {}", a.name));
            }
            let dir = a.dir.clone().unwrap_or_else(|| format!("apps/{}", a.name));
            let view = match a.slot {
                Some(_) => src.read(suite, &format!("{dir}/view.html")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(),
                None => String::new(),
            };
            let open = a.wrap.clone().unwrap_or_else(|| "<div>".into());
            let tag = tag_name(&open).ok_or_else(|| format!("{}: wrap must be one opening tag", a.name))?;
            // data-app marks ctx.root, as in the single-file build.
            let open = format!("<{tag} data-app=\"{}\"{}", a.name, &open[1 + tag.len()..]);
            let inner = format!("{open}\n{view}</{tag}>");
            let body = match a.slot.as_deref() {
                // Sections after the first carry a top rule in the suite's
                // CSS (main section:first-child has none); a hidden first
                // child keeps that look when each section is alone in a frame.
                Some("main") => {
                    let first = s.apps.iter().position(|x| x.slot.as_deref() == Some("main")) == Some(pos);
                    format!("<main>{}{inner}</main>", if first { "" } else { "<i hidden></i>" })
                }
                _ => inner,
            };
            let mut scripts = String::new();
            for rel in s.scripts.iter().chain(&a.scripts) {
                scripts.push_str(&script_tag(src, suite, rel)?);
            }
            scripts.push_str(&format!("<script>\n{SHIM_JS}\n</script>\n"));
            scripts.push_str(&script_tag(src, suite, &format!("{dir}/app.js"))?);
            (body, scripts)
        }
    };
    Ok(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{head}\
         <style>html,body{{overflow-x:hidden}} body{{display:flow-root}}</style>\n</head>\n<body>\n{body}\n\
         {scripts}<script>Kernel.start();</script>\n</body>\n</html>\n"
    ))
}
