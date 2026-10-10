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

use super::package::{safe_rel, safe_segment};
use serde::Deserialize;

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

/// The rules a page app's HTML and SVG run under (ADR-2610081003): the same sandbox as a frame, and
/// requests only to the package's own files and Wardian's page library. A sandboxed page has a
/// throwaway origin, so 'self' cannot name the server; the package is named by the request's `Host`
/// and the package's path instead. `http://` also matches `https://`, for a Wardian behind a proxy.
/// A `Host` that is not a plain host[:port] names nothing, so the page loads nothing.
pub fn page_csp(host: &str, package: &str) -> String {
    let plain = |s: &str, extra: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c) || extra.contains(c));
    let named = plain(host, ":[]") && plain(package, "");
    let own = if named { format!("http://{host}/apps/{package}/ ") } else { String::new() };
    let sdk = if named { format!("http://{host}/sdk/ ") } else { String::new() };
    let form = if named { own.trim_end() } else { "'none'" };
    format!(
        "sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads; \
         default-src 'none'; \
         script-src {own}{sdk}'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob:; \
         style-src {own}'unsafe-inline' https://fonts.googleapis.com; font-src {own}https://fonts.gstatic.com data:; \
         img-src {own}data: blob:; media-src {own}data: blob:; connect-src {own}data: blob:; \
         worker-src {own}blob:; frame-src {own}blob:; form-action {form}; base-uri 'none'; object-src 'none'"
    )
}

#[cfg(test)]
mod tests {
    use super::{frame, page_csp, script_id, script_tag, tag_name};

    #[test]
    fn tag_name_takes_only_one_complete_opening_tag() {
        assert_eq!(tag_name("<div>"), Some("div"));
        assert_eq!(tag_name("<section class=\"card\">"), Some("section"));
        assert_eq!(tag_name("<p title=\"it's\">"), Some("p"));
        for bad in ["<div", "<section", "<section><aside>", "<div></div><p>", "<div id=\"x>", "<div id='x>", "<div>text", "<div class=\"a\"><b>", "div>", "<>", "< div>"] {
            assert_eq!(tag_name(bad), None, "{bad:?} is not one opening tag");
        }
    }

    #[test]
    fn script_tag_refuses_code_that_can_escape_the_script() {
        for code in ["s = '</script>'", "s = '</SCRIPT>'", "s = '<!--<script>'", "s = '<!--'"] {
            let read = |_: &str| Some(code.as_bytes().to_vec());
            assert!(script_tag(&read, "app.js").is_err(), "{code:?} must be refused");
        }
        let read = |_: &str| Some(b"let a = 1 < 2;".to_vec());
        assert!(script_tag(&read, "app.js").is_ok());
    }

    #[test]
    fn script_id_removes_only_one_js() {
        assert_eq!(script_id("lib/app.js"), "app-src");
        assert_eq!(script_id("x.js.js"), "x.js-src");
        assert_eq!(script_id("x.js"), "x-src");
    }

    #[test]
    fn frame_refuses_two_scripts_with_one_id() {
        let suite = |scripts: &str, own: &str| {
            format!(r#"{{"scripts": [{scripts}], "apps": [{{"name": "a", "scripts": [{own}]}}]}}"#)
        };
        for (scripts, own) in [(r#""lib/app.js""#, ""), (r#""a/util.js""#, r#""b/util.js""#), (r#""x.js""#, r#""x.js.js""#)] {
            let json = suite(scripts, own);
            let read = |rel: &str| Some(if rel == "suite.json" { json.clone().into_bytes() } else { b"let a = 1;".to_vec() });
            let got = frame(&read, "", Some("a"));
            if scripts.contains("x.js") {
                assert!(got.is_ok(), "x.js and x.js.js get different ids: {got:?}");
            } else {
                assert!(got.unwrap_err().contains("both get the id"), "{scripts} and {own} must be refused");
            }
        }
    }

    /// ADR-2610100900: a glass suite's frames are see-through and share the kernel's colour scheme;
    /// a solid suite's frames are as before.
    #[test]
    fn glass_suite_frames_are_see_through() {
        let page = |surface: &str| {
            let json = format!(r#"{{"surface": {surface}, "header": "h.html", "apps": [{{"name": "a", "slot": "main"}}]}}"#);
            let read = move |rel: &str| Some(if rel == "suite.json" { json.clone().into_bytes() } else { b"<p>x</p>".to_vec() });
            (frame(&read, "", Some("a")).unwrap(), frame(&read, "", None).unwrap())
        };
        for html in <[String; 2]>::from(page(r#""glass""#)) {
            assert!(html.contains("color-scheme:light dark") && html.contains("background:transparent"), "{html}");
        }
        for surface in [r#""solid""#, r#""frosted""#, "null"] {
            for html in <[String; 2]>::from(page(surface)) {
                assert!(!html.contains("background:transparent"), "{surface}: {html}");
            }
        }
    }

    #[test]
    fn page_csp_names_only_the_package_and_the_sdk() {
        let p = page_csp("127.0.0.1:8000", "text-tools");
        assert!(p.starts_with("sandbox allow-scripts"), "the sandbox stays: {p}");
        assert!(p.contains("default-src 'none'"));
        assert!(p.contains("connect-src http://127.0.0.1:8000/apps/text-tools/ data: blob:;"));
        assert!(p.contains("script-src http://127.0.0.1:8000/apps/text-tools/ http://127.0.0.1:8000/sdk/ 'unsafe-inline'"));
        assert!(p.contains("form-action http://127.0.0.1:8000/apps/text-tools/;"));
        assert!(!p.contains("/api/") && !p.contains("'self'") && !p.contains(" * ") && !p.contains("http: ") && !p.contains("https: "));
        assert!(page_csp("[::1]:8000", "a").contains("http://[::1]:8000/apps/a/"), "IPv6 hosts work");
    }

    #[test]
    fn page_csp_with_a_strange_host_names_nothing() {
        for host in ["", "evil.com/x", "a b", "h;default-src *", "h'"] {
            let p = page_csp(host, "app");
            assert!(!p.contains("/apps/") && !p.contains("/sdk/"), "{host:?} gave {p}");
            assert!(p.contains("connect-src data: blob:;") && p.contains("form-action 'none';"), "{host:?} gave {p}");
        }
        assert!(!page_csp("127.0.0.1:8000", "a;b").contains("/apps/"), "a strange package name names nothing");
    }
}

#[derive(Deserialize)]
struct Suite {
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    scripts: Vec<String>,
    header: Option<String>,
    /// "glass" makes every frame see-through, over the kernel's light (ADR-2610100900).
    surface: Option<serde_json::Value>,
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

/// Reads a file of the suite through the reader the frame was given.
type Read<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

fn text(read: Read, rel: &str) -> Result<String, String> {
    if !safe_rel(rel) {
        return Err(format!("path not allowed: {rel}"));
    }
    let bytes = read(rel).ok_or_else(|| format!("missing file: {rel}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{rel} is not UTF-8 text"))
}

/// Inlines a script so `ctx.source("<stem>-src")` can read it back, as in the
/// single-file build. Code holding `</script` or `<!--` is refused: the first
/// ends the element early, and `<!--` then `<script` makes the parser skip the
/// closing tag added here, so the rest of the frame becomes part of this script.
fn script_tag(read: Read, rel: &str) -> Result<String, String> {
    let code = text(read, rel)?;
    if code.to_ascii_lowercase().contains("</script") {
        return Err(format!("{rel} contains </script"));
    }
    if code.contains("<!--") {
        return Err(format!("{rel} contains <!--"));
    }
    Ok(format!("<script id=\"{}\">\n{code}\n</script>\n", script_id(rel)))
}

/// The id `ctx.source` finds a script by: its file name, less one `.js`, then `-src`.
fn script_id(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    format!("{}-src", name.strip_suffix(".js").unwrap_or(name))
}

/// The tag name of an opening tag like `<section id="x">`, for its closing tag.
/// None unless `open` is exactly one opening tag: it ends with its `>` and
/// holds no other `<` or `>`, and no quote is left open (an open quote hides
/// the `>`, so the view would become part of an attribute).
pub fn tag_name(open: &str) -> Option<&str> {
    let inner = open.strip_prefix('<')?.strip_suffix('>')?;
    if inner.contains(['<', '>']) {
        return None;
    }
    let mut quote = None;
    for c in inner.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '"' || c == '\'' => quote = Some(c),
            _ => {}
        }
    }
    if quote.is_some() {
        return None;
    }
    let name = inner.split(char::is_whitespace).next()?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())).then_some(name)
}

// display:flow-root keeps child margins inside <body>, so the height the
// shim reports to the kernel is the whole of what the frame shows.

/// Builds the document for one frame: an app, or the suite header when `app` is None. `read`
/// reads the suite's files; `shim` is the script that plays `Kernel` inside the frame.
pub fn frame(read: Read, shim: &str, app: Option<&str>) -> Result<String, String> {
    let s: Suite = serde_json::from_slice(&read("suite.json").ok_or("no suite.json")?).map_err(|e| format!("suite.json: {e}"))?;

    let mut head = String::new();
    for style in &s.styles {
        if style.starts_with(FONT_CSS) {
            if style.contains(['"', '<', '>']) {
                return Err("bad font URL".into());
            }
            head.push_str(&format!("<link rel=\"stylesheet\" href=\"{style}\">\n"));
        } else {
            let css = text(read, style)?;
            if css.to_ascii_lowercase().contains("</style") {
                return Err(format!("{style} contains </style"));
            }
            head.push_str(&format!("<style>\n{css}\n</style>\n"));
        }
    }

    let (body, scripts) = match app {
        None => {
            let rel = s.header.as_deref().ok_or("suite has no header")?;
            let shim = format!("<script>\n{shim}\n</script>\n");
            (format!("<div data-app=\"header\">{}</div>", text(read, rel)?), shim)
        }
        Some(name) => {
            let pos = s.apps.iter().position(|a| a.name == name).ok_or_else(|| format!("no app \"{name}\" in suite.json"))?;
            let a = &s.apps[pos];
            if !safe_segment(&a.name) {
                return Err(format!("bad app name: {}", a.name));
            }
            let dir = a.dir.clone().unwrap_or_else(|| format!("apps/{}", a.name));
            let view = match a.slot {
                Some(_) => read(&format!("{dir}/view.html")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(),
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
            // Two scripts with one id would make ctx.source find the first one for both.
            let own = format!("{dir}/app.js");
            let rels: Vec<&String> = s.scripts.iter().chain(&a.scripts).chain([&own]).collect();
            for (i, rel) in rels.iter().enumerate() {
                if let Some(other) = rels[..i].iter().find(|o| script_id(o) == script_id(rel)) {
                    return Err(format!("{other} and {rel} both get the id {}", script_id(rel)));
                }
            }
            let mut scripts = String::new();
            for rel in &rels[..rels.len() - 1] {
                scripts.push_str(&script_tag(read, rel)?);
            }
            scripts.push_str(&format!("<script>\n{shim}\n</script>\n"));
            scripts.push_str(&script_tag(read, &own)?);
            (body, scripts)
        }
    };
    // A glass suite's frame lets the kernel's light through: no background, and the kernel's colour
    // scheme, or the browser puts an opaque backdrop behind the frame. A pane's outer shadow would be
    // cut off at the frame's edge, so it has none (ADR-2610100900).
    let glass = if s.surface.as_ref().and_then(|v| v.as_str()) == Some("glass") {
        "<style>:root{color-scheme:light dark} html,body{background:transparent!important} :root{--w-glass-shadow:0 0 #0000}</style>\n"
    } else {
        ""
    };
    Ok(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{head}{glass}\
         <style>html,body{{overflow-x:hidden}} body{{display:flow-root}}</style>\n</head>\n<body>\n{body}\n\
         {scripts}<script>Kernel.start();</script>\n</body>\n</html>\n"
    ))
}
