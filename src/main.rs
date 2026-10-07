mod ai;
mod check;
mod docs;
mod hub;
mod import;
mod new;
mod suite;
mod source;
mod splunk;
mod ui;

use ai::Studio;
use hub::Hub;
use splunk::Splunk;
use serde_json::{json, Value};
use source::{safe_rel, safe_segment};
use std::{io::Read, path::{Path, PathBuf}, sync::Arc, thread, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("../static/index.html");
const KERNEL_HTML: &str = include_str!("../static/kernel.html");
const LOGO_SVG: &str = include_str!("../static/logo.svg");
const CHANNELS_JS: &str = include_str!("../static/channels.js");
/// Channels for page apps, and the standard components (the same ones suite frames get).
const SDK_JS: &str = concat!(include_str!("../static/sdk.js"), "\n", include_str!("../static/ui/progress.js"));
const MAX_BODY_BYTES: u64 = 64 * 1024;

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("wasm") => "application/wasm", // required for instantiateStreaming
        Some("json") => "application/json",
        Some("js" | "mjs") => "text/javascript",
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt" | "md" | "ts" | "rs" | "toml" | "sh") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A file from inside an app. App pages are code from whoever made the zip,
/// so every page is served sandboxed: the browser gives it a throwaway
/// origin, and it cannot call this server's settings API or read its
/// replies, even when opened in its own tab. The CORS header lets such a
/// sandboxed page still load its own scripts and .wasm files from here.
fn app_file(bytes: Vec<u8>, rel: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let ct = mime(Path::new(rel));
    let mut resp = Response::from_data(bytes)
        .with_header(header("Content-Type", ct))
        .with_header(header("Cache-Control", "no-cache"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Access-Control-Allow-Origin", "*"));
    if ct.starts_with("text/html") || ct == "image/svg+xml" {
        resp = resp.with_header(header(
            "Content-Security-Policy",
            "sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads",
        ));
    }
    resp
}

/// Reports a page's errors to the window that opened it, for the browser
/// test after an AI change. Added only when the URL asks for it.
const PROBE: &str = r#"<script>(()=>{const p=m=>parent.postMessage({wardianProbe:1,message:String(m)},'*');
addEventListener('error',e=>{const t=e.target;if(t&&t!==window&&(t.src||t.href)){p('could not load '+(t.src||t.href).split('/').pop());return}
p((e.message||'error')+(e.filename?' ('+e.filename.split('/').pop()+':'+e.lineno+')':''))},true);
addEventListener('unhandledrejection',e=>p('unhandled promise rejection: '+(e.reason&&e.reason.message||e.reason)));
const ce=console.error;console.error=(...a)=>{p('console.error: '+a.map(x=>x&&x.message||String(x)).join(' '));ce.apply(console,a)};
addEventListener('load',()=>setTimeout(()=>p('__loaded'),0))})()</script>"#;

fn with_probe(bytes: Vec<u8>) -> Vec<u8> {
    // The page has a throwaway origin, so the browser hides the details of
    // errors in its own scripts ("Script error.") unless they are loaded
    // with CORS, which app files allow.
    let html = String::from_utf8_lossy(&bytes).replace("<script src", "<script crossorigin src");
    let at = html.find("<head>").map(|i| i + 6).unwrap_or(0);
    format!("{}{PROBE}{}", &html[..at], &html[at..]).into_bytes()
}

fn req_header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

fn query<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split_once('?')?.1.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key && !v.is_empty()).then_some(v)
    })
}

fn same_bytes(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Who may change settings and browse Drive. With ADMIN_TOKEN set, whoever
/// sends it in `X-Admin-Token`. Without it, only a browser on this machine:
/// the peer must be loopback, and the Host header must name loopback too, so
/// a hostile web page cannot reach the API through DNS rebinding.
fn is_admin(req: &Request) -> bool {
    if let Some(token) = env("ADMIN_TOKEN") {
        return req_header(req, "X-Admin-Token").is_some_and(|t| same_bytes(t, &token));
    }
    let peer_local = req.remote_addr().is_some_and(|a| a.ip().is_loopback());
    let host = req_header(req, "Host").unwrap_or("");
    let host = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    };
    peer_local && matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn json_resp(code: u16, v: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(v.to_string().into_bytes())
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn result_resp(r: Result<Value, String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(v) => json_resp(200, v),
        Err(e) => json_resp(400, json!({ "error": e })),
    }
}

/// Reads a JSON body. Requiring the JSON content type also blocks plain HTML
/// form posts from other sites, since browsers must ask first (CORS
/// preflight) before sending JSON, and this server never says yes.
fn read_json(req: &mut Request) -> Result<Value, String> {
    let ct = req_header(req, "Content-Type").unwrap_or("");
    if !ct.starts_with("application/json") {
        return Err("expected Content-Type: application/json".into());
    }
    let mut body = String::new();
    req.as_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_BODY_BYTES {
        return Err("body too large".into());
    }
    serde_json::from_str(&body).map_err(|e| format!("invalid JSON: {e}"))
}

/// Reads an uploaded zip. Like the JSON check above, the zip content type is
/// one a browser will not send across sites without asking first.
fn read_zip(req: &mut Request) -> Result<Vec<u8>, String> {
    if req_header(req, "Content-Type") != Some("application/zip") {
        return Err("expected Content-Type: application/zip".into());
    }
    let mut body = Vec::new();
    req.as_reader()
        .take(hub::MAX_ZIP_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > hub::MAX_ZIP_BYTES {
        return Err("the zip is larger than 100 MB".into());
    }
    Ok(body)
}

fn api_post(path: &str, body: Value, hub: &Arc<Hub>, studio: &Arc<Studio>, splunk: &Arc<Splunk>) -> Result<Value, String> {
    match path {
        "/api/splunk/config" => splunk.set_config(&body),
        "/api/splunk/search" => {
            // The kernel asks for an app of a suite. Check here too, not only in the
            // browser: the app must declare "splunk" and the user must have allowed it.
            let package = body["package"].as_str().unwrap_or("");
            let app = body["app"].as_str().unwrap_or("");
            hub.check_host_cap(package, app, "splunk", "splunk")?;
            let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
            let out = splunk.search(&s("search"), &s("earliest"), &s("latest"));
            println!("splunk: {package}/{app} ran a search: {}", if out.is_ok() { "ok" } else { "failed" });
            out
        }
        "/api/grants" => hub.set_grant(&body),
        "/api/ai/key" => studio.set_key(body["key"].as_str().unwrap_or(""), body["workspace"].as_str()),
        "/api/ai/send" => studio.send(hub, &body),
        "/api/ai/sample" => {
            // claude:sample for a suite app, billed to the saved Anthropic key. Same checks as splunk.
            let package = body["package"].as_str().unwrap_or("");
            let app = body["app"].as_str().unwrap_or("");
            hub.check_host_cap(package, app, "claude:sample", "ai").map_err(|e| format!("not_granted: {e}"))?;
            let out = studio.sample(&body);
            println!("ai: {package}/{app} asked Claude: {}", match &out { Ok(_) => "ok".to_string(), Err(e) => e.clone() });
            out
        }
        "/api/ai/stop" => studio.stop(body["session"].as_str().unwrap_or("")),
        "/api/ai/claim-test" => studio.claim_test(body["session"].as_str().unwrap_or(""), body["saved"].as_u64().unwrap_or(u64::MAX) as usize),
        "/api/ai/tested" => studio.tested(body["session"].as_str().unwrap_or(""), &body),
        "/api/refresh" => {
            hub.refresh()?;
            Ok(hub.status())
        }
        "/api/drive/key" => {
            // The body is the key file exactly as Google issued it.
            let email = hub.set_key(&body.to_string())?;
            Ok(json!({ "client_email": email }))
        }
        "/api/apps/remove" => hub.remove_app(body["name"].as_str().unwrap_or("")),
        "/api/apps/restore" => hub.restore_app(body["id"].as_str().unwrap_or("")),
        "/api/import/url" => {
            let url = body["url"].as_str().unwrap_or("").trim();
            hub.import_url(url, body["replace"].as_bool().unwrap_or(false))
        }
        "/api/source" => {
            match body["kind"].as_str() {
                Some("local") => hub.use_local()?,
                Some("drive") => {
                    let id = body["folder_id"].as_str().unwrap_or("");
                    let name = body["folder_name"].as_str().filter(|n| !n.is_empty()).unwrap_or(id);
                    hub.use_drive(id, name)?
                }
                _ => return Err("kind must be \"local\" or \"drive\"".into()),
            }
            Ok(hub.status())
        }
        _ => Err("not found".into()),
    }
}

fn handle(mut req: Request, hub: &Arc<Hub>, studio: &Arc<Studio>, splunk: &Arc<Splunk>) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let admin = is_admin(&req);

    let resp = match (req.method().clone(), parts.as_slice()) {
        (Method::Post, ["api", "import"]) if admin => result_resp(read_zip(&mut req).and_then(|bytes| {
            let name = query(&url, "name").unwrap_or("imported.zip");
            hub.import(&bytes, name, query(&url, "replace") == Some("1"))
        })),
        (Method::Post, ["api", ..]) => {
            if !admin {
                json_resp(403, json!({ "error": "settings are locked; see ADMIN_TOKEN in the README" }))
            } else {
                result_resp(read_json(&mut req).and_then(|body| api_post(path, body, hub, studio, splunk)))
            }
        }
        // no-cache: the page and the API change together, so an old page
        // must never run against a newer server.
        (Method::Get, [""]) => Response::from_string(INDEX_HTML)
            .with_header(header("Content-Type", mime(Path::new("x.html"))))
            .with_header(header("Cache-Control", "no-cache")),
        // /api/apps keeps its original reply (names only), so a page loaded
        // before an upgrade keeps working; app-list adds titles and pages.
        // Documentation, and the JSON Schemas editors use to check app.json and suite.json.
        (Method::Get, ["docs"]) => Response::from_string("")
            .with_status_code(302)
            .with_header(header("Location", "/docs/guide")),
        (Method::Get, ["docs", name]) => match docs::page(name) {
            Some(html) => Response::from_string(html)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-cache")),
            None => Response::from_string("not found").with_status_code(404),
        },
        (Method::Get, ["schemas", file]) => match *file {
            "app.schema.json" | "suite.schema.json" => {
                let body = if *file == "app.schema.json" { docs::APP_SCHEMA } else { docs::SUITE_SCHEMA };
                Response::from_string(body)
                    .with_header(header("Content-Type", "application/schema+json"))
                    .with_header(header("Access-Control-Allow-Origin", "*"))
                    .with_header(header("Cache-Control", "no-cache"))
            }
            _ => Response::from_string("not found").with_status_code(404),
        },
        // Browsers ask for a tab icon on every page; "no content" keeps 404s out of the console.
        (Method::Get, ["favicon.ico"]) => Response::from_data(Vec::new()).with_status_code(204),
        // channels.js runs in Wardian's own pages; sdk/wardian.js is for page apps, which have a
        // throwaway origin, so it carries the CORS header like app files. sdk/rustle.js is its
        // name from before the rename.
        (Method::Get, ["channels.js"]) => Response::from_string(CHANNELS_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["sdk", "wardian.js" | "rustle.js"]) => Response::from_string(SDK_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header("Cache-Control", "no-cache")),
        // The component library: the gallery, and each file, for `wardian add` users to read.
        (Method::Get, ["ui"]) => Response::from_string(ui::GALLERY)
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["ui", f]) => match ui::file(f) {
            Some(body) => Response::from_string(body)
                .with_header(header("Content-Type", mime(Path::new(f))))
                .with_header(header("Cache-Control", "no-cache")),
            None => Response::from_string("not found").with_status_code(404),
        },
        (Method::Get, ["api", "grants"]) => json_resp(200, hub.grants()),
        (Method::Get, ["logo.svg"]) => Response::from_string(LOGO_SVG)
            .with_header(header("Content-Type", "image/svg+xml"))
            .with_header(header("Cache-Control", "max-age=86400")),
        (Method::Get, ["api", "apps"]) => json_resp(200, json!(hub.source().list_apps())),
        (Method::Get, ["api", "app-list"]) => json_resp(200, json!(hub.source().apps())),
        (Method::Get, ["api", "status"]) => {
            let mut s = hub.status();
            s["admin"] = json!(admin);
            s["ai"] = studio.status();
            s["splunk"] = splunk.status();
            json_resp(200, s)
        }
        (Method::Get, ["api", "trash"]) if admin => json_resp(200, hub.trash()),
        (Method::Get, ["api", "ai", "sessions"]) if admin => json_resp(200, studio.sessions()),
        (Method::Get, ["api", "ai", "events"]) if admin => {
            let since = query(&url, "since").and_then(|n| n.parse().ok()).unwrap_or(0);
            result_resp(studio.events(query(&url, "session").unwrap_or(""), since))
        }
        (Method::Get, ["api", "drive", "browse"]) if admin => {
            result_resp(hub.browse(query(&url, "parent")))
        }
        (Method::Get, ["api", "drive", "preview"]) if admin => {
            result_resp(hub.preview(query(&url, "folder").unwrap_or("")))
        }
        (Method::Get, ["api", "drive", _]) => json_resp(403, json!({ "error": "settings are locked" })),
        // A suite: the host kernel page, and one sandboxed frame per app.
        (Method::Get, ["run", name]) if safe_segment(name) => Response::from_string(KERNEL_HTML)
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["frame", name, app @ ..]) if safe_segment(name) && app.len() <= 1 => {
            let app = app.first().copied().filter(|a| !a.is_empty());
            match suite::frame(&hub.source(), name, app) {
                Ok(html) => Response::from_string(html)
                    .with_header(header("Content-Type", "text/html; charset=utf-8"))
                    .with_header(header("Content-Security-Policy", suite::FRAME_CSP))
                    .with_header(header("X-Content-Type-Options", "nosniff"))
                    .with_header(header("Cache-Control", "no-cache")),
                Err(e) => Response::from_string(format!("cannot build frame: {e}")).with_status_code(404),
            }
        }
        (Method::Get, ["apps", name, rest @ ..]) if safe_segment(name) => {
            // "/apps/x/demo/" means "/apps/x/demo/index.html".
            let mut rel = rest.join("/");
            if path.ends_with('/') {
                rel = if rel.is_empty() { "index.html".into() } else { format!("{rel}/index.html") };
            }
            let bytes = if safe_rel(&rel) { hub.source().read(name, &rel) } else { None };
            match bytes {
                Some(bytes) if query(&url, "wardian-probe") == Some("1") && rel.ends_with(".html") => app_file(with_probe(bytes), &rel),
                Some(bytes) => app_file(bytes, &rel),
                None => Response::from_string("not found").with_status_code(404),
            }
        }
        (Method::Get, _) => Response::from_string("not found").with_status_code(404),
        _ => Response::from_string("method not allowed").with_status_code(405),
    };
    if let Err(e) = req.respond(resp) {
        eprintln!("response error: {e}");
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

const USAGE: &str = "Wardian — runs WebAssembly apps and suites in the browser

usage:
  wardian [APPS_FOLDER]          serve the apps in APPS_FOLDER (default: ./apps)
  wardian new KIND PATH          create a starter package: module, page or suite
  wardian add COMPONENT... PATH  copy UI components (button, tabs, dialog…) into a package; --list shows them
  wardian check PACKAGE...       check packages (folders, .zip or .wardian files) against SPEC.md
  wardian --version

settings come from environment variables; see README.md";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => std::process::exit(check::run(&args[1..])),
        Some("new") => std::process::exit(new::run(&args[1..])),
        Some("add") => std::process::exit(ui::run(&args[1..])),
        Some("--version" | "-V") => {
            println!("Wardian {} (package format {})", env!("CARGO_PKG_VERSION"), source::FORMAT);
            return;
        }
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            return;
        }
        Some(a) if a.starts_with('-') => {
            eprintln!("unknown option {a}\n\n{USAGE}");
            std::process::exit(2);
        }
        _ => {}
    }
    let local_root = PathBuf::from(args.first().cloned().unwrap_or_else(|| "apps".into()));
    let data_dir = PathBuf::from(env("DATA_DIR").unwrap_or_else(|| "data".into()));
    let api_base = env("GDRIVE_API_BASE").unwrap_or_else(|| "https://www.googleapis.com".into());
    let secs: u64 = env("REFRESH_SECS").and_then(|s| s.parse().ok()).unwrap_or(60);

    let studio = Arc::new(Studio::new(&data_dir));
    let splunk = Arc::new(Splunk::new(&data_dir));
    let hub = Arc::new(Hub::new(data_dir, local_root, api_base, Duration::from_secs(secs.max(5))));
    hub.start(env("GDRIVE_SA_KEY"), env("GDRIVE_FOLDER_ID"));

    let addr = env("ADDR").unwrap_or_else(|| "127.0.0.1:8000".into());
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Wardian: cannot listen on {addr}: {e}");
            eprintln!("Another program (perhaps another Wardian) is using that address.");
            eprintln!("Stop it, or pick another port, e.g. ADDR=127.0.0.1:8001 wardian");
            std::process::exit(1);
        }
    };
    // Print the address actually bound: with port 0 the system picks a free port.
    match server.server_addr().to_ip() {
        Some(bound) => println!("listening on http://{bound}"),
        None => println!("listening on http://{addr}"),
    }
    if env("ADMIN_TOKEN").is_none() {
        println!("settings: only from a browser on this machine (set ADMIN_TOKEN to allow others)");
    }

    for req in server.incoming_requests() {
        let (hub, studio, splunk) = (Arc::clone(&hub), Arc::clone(&studio), Arc::clone(&splunk));
        thread::spawn(move || handle(req, &hub, &studio, &splunk));
    }
}
