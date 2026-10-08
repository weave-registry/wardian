//! The example apps on the static website (ADR-2610081900): a "Try it" frame on each example's
//! page for the apps that need no Wardian server, and a download of every example. `wardian docs`
//! writes the apps' files at the paths Wardian serves them from (`/apps/<name>/`), a form page for
//! each module app, each runnable suite's kernel page and frames (`/run/<name>/`, `/frame/<name>/`),
//! a zip of each example, and the headers the website sends with them.

use crate::domain::export;
use crate::domain::suite::{self, FRAME_CSP};
use crate::ports::assets::Assets;
use std::io::{Cursor, Write};

/// What an example needs to run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// WebAssembly functions only: Wardian draws the form.
    Module,
    /// A page of its own. `channels`: it talks to other apps, which needs a Wardian.
    Page { channels: bool },
    /// Parts run by Wardian's kernel. `browser_only`: every capability it declares works with
    /// no server behind the kernel (storage then stays in the browser, Claude is off).
    Suite { browser_only: bool },
}

impl Kind {
    /// Runs in a plain web page, with no Wardian behind it.
    pub fn runs_in_a_browser(self) -> bool {
        matches!(self, Kind::Module | Kind::Page { channels: false } | Kind::Suite { browser_only: true })
    }
}

/// The capabilities a suite may declare and still run on the website: storage falls back to the
/// browser, and `claude:sample` resolves to null without a server, which every app must handle.
const BROWSER_CAPS: [&str; 6] = ["storage", "asset", "worker", "source", "claude:downloads", "claude:sample"];

fn files_of<'a>(assets: &'a dyn Assets, app: &str) -> impl Iterator<Item = (&'a str, &'a [u8])> + 'a {
    let prefix = format!("{app}/");
    assets.example_apps().iter().filter_map(move |(p, b)| p.strip_prefix(prefix.as_str()).map(|rel| (rel, *b)))
}

fn manifest(assets: &dyn Assets, app: &str) -> serde_json::Value {
    files_of(assets, app).find(|(rel, _)| *rel == "app.json").and_then(|(_, b)| serde_json::from_slice(b).ok()).unwrap_or_default()
}

/// The example's kind, or None when `app` is not an example.
pub fn kind(assets: &dyn Assets, app: &str) -> Option<Kind> {
    let mut files = files_of(assets, app).map(|(rel, _)| rel).peekable();
    files.peek()?;
    if files.any(|rel| rel == "suite.json") {
        let s: serde_json::Value = files_of(assets, app).find(|(rel, _)| *rel == "suite.json").and_then(|(_, b)| serde_json::from_slice(b).ok()).unwrap_or_default();
        let parts = s["apps"].as_array().cloned().unwrap_or_default();
        let caps_ok = parts.iter().flat_map(|a| a["caps"].as_array().cloned().unwrap_or_default()).all(|c| c.as_str().is_some_and(|c| BROWSER_CAPS.contains(&c)));
        let no_channels = s["channels"].is_null() && parts.iter().all(|a| a["channels"].is_null());
        return Some(Kind::Suite { browser_only: caps_ok && no_channels });
    }
    let m = manifest(assets, app);
    Some(match m["page"].as_str() {
        Some(_) => Kind::Page { channels: !m["channels"].is_null() },
        None => Kind::Module,
    })
}

/// The example apps' names.
pub fn names(assets: &dyn Assets) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = assets.example_apps().iter().filter_map(|(p, _)| p.split('/').next()).collect();
    names.dedup();
    names
}

/// Where the website shows an example running: its page, or the form page of a module.
fn try_src(assets: &dyn Assets, app: &str) -> Option<String> {
    match kind(assets, app)? {
        Kind::Module => Some(format!("/docs/try/{app}/")),
        Kind::Page { channels: false } => manifest(assets, app)["page"].as_str().map(|p| format!("/apps/{app}/{p}")),
        Kind::Suite { browser_only: true } => Some(format!("/run/{app}/")),
        _ => None,
    }
}

/// The box at the top of an example's page on the website: the app running, or why it cannot here,
/// and its download.
pub fn try_box(assets: &dyn Assets, app: &str) -> Option<String> {
    let k = kind(assets, app)?;
    let download = format!(
        "<p><a class=\"w-button\" href=\"/downloads/{app}.wardian\" download>Download {app}.wardian</a> \
         <span class=\"muted\">Import it in Wardian: Settings → Import.</span></p>"
    );
    let body = match (try_src(assets, app), k) {
        (Some(src), _) => format!(
            "<iframe class=\"try-frame\" src=\"{src}\" title=\"{app}, running\" loading=\"lazy\" \
             sandbox=\"allow-scripts allow-same-origin allow-forms allow-downloads allow-modals\"></iframe>\
             <p class=\"muted\">The app runs here in your browser. Nothing you type leaves it{kept}. \
             <a href=\"{src}\" target=\"_blank\" rel=\"noopener\">Open it on its own</a>.</p>",
            kept = if matches!(k, Kind::Suite { .. }) { "; what it saves stays in this browser, and Claude is off here" } else { "" }
        ),
        (None, Kind::Suite { .. }) => "<p>This suite uses its own database, Splunk or channels, which need a Wardian. Download it \
             and import it in Wardian to run it.</p>"
            .to_string(),
        (None, _) => "<p>This app talks to other apps over channels, which needs a Wardian. Download it and import it in \
             Wardian to run it.</p>"
            .to_string(),
    };
    Some(format!("<section class=\"try\" aria-label=\"Try {app}\"><h2 id=\"try-it\">Try it</h2>{body}{download}</section>"))
}

/// The app's title and one-line description, from app.json or suite.json.
fn title_and_line(assets: &dyn Assets, app: &str) -> (String, String) {
    let m = match kind(assets, app) {
        Some(Kind::Suite { .. }) => files_of(assets, app).find(|(rel, _)| *rel == "suite.json").and_then(|(_, b)| serde_json::from_slice(b).ok()).unwrap_or_default(),
        _ => manifest(assets, app),
    };
    let title = m["title"].as_str().unwrap_or(app).to_string();
    // Without a description, the README's first sentence.
    let readme = || {
        files_of(assets, app)
            .find(|(rel, _)| *rel == "README.md")
            .map(|(_, b)| String::from_utf8_lossy(b).lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let text = m["description"].as_str().map(String::from).unwrap_or_else(readme).replace(['`', '*'], "");
    let line = text.split(". ").next().unwrap_or("").trim_end_matches('.').to_string();
    (title, line)
}

/// The gallery at the top of the website's examples page: every example, with Try it for the ones
/// that run in the browser and Download for every one.
pub fn gallery(assets: &dyn Assets) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut apps: Vec<(&str, Kind)> = names(assets).into_iter().filter_map(|n| kind(assets, n).map(|k| (n, k))).collect();
    // The ones that run here first, then the rest; each group keeps the build's order.
    apps.sort_by_key(|(_, k)| !k.runs_in_a_browser());
    let runnable = apps.iter().filter(|(_, k)| k.runs_in_a_browser()).count();
    let cards: String = apps
        .iter()
        .map(|(app, k)| {
            let (title, line) = title_and_line(assets, app);
            let what = match k {
                Kind::Module => "module",
                Kind::Page { .. } => "page",
                Kind::Suite { .. } => "suite",
            };
            let act = if k.runs_in_a_browser() {
                format!("<a class=\"go\" href=\"/docs/examples/{app}/#try-it\">Try it</a>")
            } else {
                format!("<span class=\"needs\">Needs Wardian</span> <a href=\"/downloads/{app}.wardian\" download>Download</a>")
            };
            format!(
                "<li><a class=\"card-link\" href=\"/docs/examples/{app}/\"><strong>{}</strong><small>{what}</small><span>{}</span></a>{act}</li>",
                esc(&title),
                esc(&line)
            )
        })
        .collect();
    format!(
        "<section class=\"gallery\" id=\"try\" aria-label=\"Try the examples\"><h2>Try them in your browser</h2>\
         <p>{runnable} of the {} examples run right here, with nothing to install. The others need a Wardian: download one and \
         import it.</p><ul>{cards}</ul></section>",
        apps.len()
    )
}

/// A module's form page: each exported function with an input per parameter and a Run button,
/// as Wardian draws it.
fn module_page(app: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{app}</title>
<link rel="stylesheet" href="/ui/theme.css"><link rel="stylesheet" href="/ui/button.css"><link rel="stylesheet" href="/ui/field.css">
<style>
  body {{ font-family: system-ui, sans-serif; margin: 0; padding: 16px; background: var(--w-bg, #fff); color: var(--w-fg, #111); }}
  .fn {{ display: flex; flex-wrap: wrap; gap: .5rem; align-items: center; padding: .5rem 0; border-bottom: 1px solid var(--w-border, #ddd); }}
  .fn code {{ min-width: 10rem; font-weight: 600; }}
  .fn input {{ width: 7rem; }}
  .out {{ font-variant-numeric: tabular-nums; font-weight: 600; }}
  .err {{ color: #c8372d; }}
</style></head>
<body><p id="msg">Loading the module…</p><div id="fns"></div>
<script>
// Wardian's module form: one row per exported function, an input per parameter. WebAssembly does
// not say a parameter's type, so a number is tried first and a BigInt (for i64) after.
const el = (t, p = {{}}, ...k) => {{ const n = Object.assign(document.createElement(t), p); n.append(...k); return n; }};
function call(fn, raw) {{
  const nums = raw.map((s) => (s.trim() === '' ? 0 : Number(s)));
  if (nums.some(Number.isNaN)) throw new Error('every argument must be a number');
  try {{ return fn(...nums); }}
  catch (e) {{ if (e instanceof TypeError && /BigInt/i.test(e.message)) return fn(...raw.map((s) => BigInt(s.trim() || '0'))); throw e; }}
}}
(async () => {{
  try {{
    const bytes = await (await fetch('/apps/{app}/app.wasm')).arrayBuffer();
    const mod = await WebAssembly.compile(bytes);
    const imports = {{}};
    for (const i of WebAssembly.Module.imports(mod)) {{ imports[i.module] ??= {{}}; if (i.kind === 'function') imports[i.module][i.name] = () => 0; }}
    const inst = await WebAssembly.instantiate(mod, imports);
    const fns = Object.entries(inst.exports).filter(([k, v]) => typeof v === 'function' && !/^(alloc|dealloc|__)/.test(k));
    document.getElementById('msg').textContent = fns.length + ' function(s). Type numbers and press Run.';
    for (const [name, fn] of fns) {{
      const ins = Array.from({{ length: fn.length }}, (_, i) => el('input', {{ className: 'w-input', inputMode: 'decimal', ariaLabel: name + ' argument ' + (i + 1), value: '0' }}));
      const out = el('span', {{ className: 'out' }});
      const run = el('button', {{ className: 'w-button', textContent: 'Run', ariaLabel: 'Run ' + name, onclick: () => {{
        try {{ out.className = 'out'; out.textContent = '= ' + String(call(fn, ins.map((i) => i.value))); }}
        catch (e) {{ out.className = 'out err'; out.textContent = e.message; }}
      }} }});
      document.getElementById('fns').append(el('div', {{ className: 'fn' }}, el('code', {{ textContent: name }}), ...ins, run, out));
    }}
  }} catch (e) {{ document.getElementById('msg').textContent = 'The module did not load: ' + e.message; }}
}})();
</script></body></html>
"#
    )
}

/// The headers the website sends with the apps (Vercel's `vercel.json`): no request may leave the
/// website, no form may post anywhere, and only the website may frame them.
fn headers() -> String {
    let csp = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob:; \
               style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com data:; \
               connect-src 'self' data: blob:; form-action 'none'; base-uri 'none'; object-src 'none'; frame-ancestors 'self'";
    let v = serde_json::json!({ "headers": [
        { "source": "/apps/(.*)", "headers": [
            { "key": "Content-Security-Policy", "value": csp },
            { "key": "X-Content-Type-Options", "value": "nosniff" } ] },
        { "source": "/docs/try/(.*)", "headers": [
            { "key": "Content-Security-Policy", "value": csp } ] },
        { "source": "/frame/(.*)", "headers": [
            { "key": "Content-Security-Policy", "value": FRAME_CSP },
            { "key": "X-Content-Type-Options", "value": "nosniff" } ] },
        { "source": "/downloads/(.*)", "headers": [
            { "key": "Content-Disposition", "value": "attachment" },
            { "key": "Content-Type", "value": export::MIME } ] }
    ] });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

/// One example as a `.wardian` file (SPEC.md 7.2): the package folder and the manifest a Wardian
/// export carries, with no data. The same every time for the same files and version, so the
/// website's copy changes only when the app or Wardian's version does.
fn wardian_file_of(assets: &dyn Assets, app: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for (rel, bytes) in files_of(assets, app) {
        zip.start_file(format!("{app}/{rel}"), opts).expect("a zip entry");
        zip.write_all(bytes).expect("a zip entry");
    }
    let title = title(assets, app);
    let manifest = export::manifest(app, title.as_deref(), 0, env!("CARGO_PKG_VERSION"), &export::DataIncluded::default());
    zip.start_file(format!("{app}/.wardian/export.json"), opts).expect("a zip entry");
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes()).expect("a zip entry");
    zip.finish().expect("a zip").into_inner()
}

/// The example's title, from its `suite.json` or `app.json`.
fn title(assets: &dyn Assets, app: &str) -> Option<String> {
    files_of(assets, app)
        .find(|(rel, _)| *rel == "suite.json" || *rel == "app.json")
        .and_then(|(_, b)| serde_json::from_slice::<serde_json::Value>(b).ok())
        .and_then(|v| v["title"].as_str().map(str::to_string))
}

/// Every file the website needs for the examples, as (path, bytes).
pub fn site_files(assets: &dyn Assets) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for app in names(assets) {
        let k = kind(assets, app);
        if k.is_some_and(Kind::runs_in_a_browser) {
            out.extend(files_of(assets, app).map(|(rel, b)| (format!("apps/{app}/{rel}"), b.to_vec())));
        }
        if k == Some(Kind::Module) {
            out.push((format!("docs/try/{app}/index.html"), module_page(app).into_bytes()));
        }
        if k == Some(Kind::Suite { browser_only: true }) {
            out.extend(suite_files(assets, app));
        }
        out.push((format!("downloads/{app}.wardian"), wardian_file_of(assets, app)));
    }
    for name in ["state.js", "channels.js"] {
        out.push((name.to_string(), assets.host_file(name).unwrap_or_default().as_bytes().to_vec()));
    }
    out.push(("vercel.json".to_string(), headers().into_bytes()));
    out
}

/// A suite as Wardian serves it: the kernel page at `/run/<name>/`, and each frame the kernel asks
/// for, built now as the server would build it on request (`/frame/<name>/` for the header,
/// `/frame/<name>/<part>/` for each part).
fn suite_files(assets: &dyn Assets, app: &str) -> Vec<(String, Vec<u8>)> {
    let read = |rel: &str| files_of(assets, app).find(|(r, _)| *r == rel).map(|(_, b)| b.to_vec());
    let mut out = vec![(format!("run/{app}/index.html"), assets.host_file("kernel.html").unwrap_or_default().as_bytes().to_vec())];
    if let Ok(header) = suite::frame(&read, assets.frame_shim(), None) {
        out.push((format!("frame/{app}/index.html"), header.into_bytes()));
    }
    let s: serde_json::Value = read("suite.json").and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    for part in s["apps"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()) {
        if let Ok(html) = suite::frame(&read, assets.frame_shim(), Some(part)) {
            out.push((format!("frame/{app}/{part}/index.html"), html.into_bytes()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::secondary::embedded_assets::Embedded;

    #[test]
    fn demos_kinds_of_the_examples() {
        let a = Embedded;
        assert_eq!(kind(&a, "adder"), Some(Kind::Module));
        assert_eq!(kind(&a, "life"), Some(Kind::Page { channels: false }));
        assert_eq!(kind(&a, "focus-timer"), Some(Kind::Page { channels: true }));
        assert_eq!(kind(&a, "usl-lab"), Some(Kind::Suite { browser_only: false }));
        assert_eq!(kind(&a, "loan-planner"), Some(Kind::Suite { browser_only: true }));
        assert_eq!(kind(&a, "meeting-notes"), Some(Kind::Suite { browser_only: true }), "Claude is off without a server");
        assert_eq!(kind(&a, "nothing"), None);
        let runnable: Vec<&str> = names(&a).into_iter().filter(|n| kind(&a, n).is_some_and(Kind::runs_in_a_browser)).collect();
        assert_eq!(runnable.len(), 11, "{runnable:?}");
    }

    #[test]
    fn demos_each_download_is_a_wardian_file_import_reads() {
        let a = Embedded;
        for app in names(&a) {
            let bytes = wardian_file_of(&a, app);
            let mut z = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).expect("a zip");
            let mut m = Vec::new();
            std::io::Read::read_to_end(&mut z.by_name(&format!("{app}/.wardian/export.json")).expect("a manifest"), &mut m).unwrap();
            let read = export::read_manifest(&m).expect("Wardian reads the manifest");
            assert_eq!((read.package.as_str(), read.data), (app, false), "{app}: the app, without data");
            assert_eq!(read.value["title"].as_str(), title(&a, app).as_deref(), "{app}: its title");
            assert!(z.file_names().all(|n| n.starts_with(&format!("{app}/"))), "{app}: one package folder at the top");
        }
        assert!(headers().contains(export::MIME), "served as a .wardian file");
    }

    #[test]
    fn demos_every_example_has_a_download_and_each_runnable_one_its_files() {
        let a = Embedded;
        let files = site_files(&a);
        let has = |p: &str| files.iter().any(|(q, _)| q == p);
        for app in names(&a) {
            assert!(has(&format!("downloads/{app}.wardian")), "{app} has a download");
            let b = try_box(&a, app).unwrap();
            assert!(b.contains(&format!("/downloads/{app}.wardian")), "{app}'s page links its download");
            match kind(&a, app).unwrap() {
                Kind::Module => assert!(has(&format!("docs/try/{app}/index.html")) && has(&format!("apps/{app}/app.wasm")) && b.contains("<iframe")),
                Kind::Page { channels: false } => {
                    let page = manifest(&a, app)["page"].as_str().unwrap().to_string();
                    assert!(has(&format!("apps/{app}/{page}")) && b.contains(&format!("/apps/{app}/{page}")), "{app}");
                }
                Kind::Suite { browser_only: true } => {
                    assert!(has(&format!("run/{app}/index.html")) && has(&format!("frame/{app}/index.html")) && has(&format!("apps/{app}/suite.json")), "{app}");
                    assert!(b.contains(&format!("/run/{app}/")), "{app}");
                }
                _ => assert!(!b.contains("<iframe") && !has(&format!("apps/{app}/suite.json")), "{app} does not run on the website"),
            }
        }
        assert!(has("vercel.json"));
    }

    #[test]
    fn demos_a_download_is_the_same_each_time_and_holds_the_app() {
        let a = Embedded;
        let (one, two) = (wardian_file_of(&a, "life"), wardian_file_of(&a, "life"));
        assert_eq!(one, two, "the website's copy changes only when the app does");
        let mut z = zip::ZipArchive::new(Cursor::new(one)).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.iter().all(|n| n.starts_with("life/")) && names.contains(&"life/app.json".to_string()), "{names:?}");
    }

    #[test]
    fn demos_the_website_keeps_apps_off_the_network() {
        let h = headers();
        assert!(h.contains("connect-src 'self' data: blob:") && h.contains("form-action 'none'") && h.contains("frame-ancestors 'self'"));
        assert!(h.contains("\"/apps/(.*)\"") && h.contains("\"/docs/try/(.*)\""));
    }
}
