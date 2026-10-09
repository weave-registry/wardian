//! The web server: Wardian's own pages, the JSON API they use, and the apps' files. It knows
//! the driving ports only; what each request does is the use cases' business.

use crate::ports::service::{export_file_name, page_csp, safe_rel, safe_segment, JobKind, Services, Unwatched, EXPORT_MIME, FRAME_CSP, MAX_ZIP_BYTES};
use serde_json::{json, Value};
use super::http_server::{Header, Method, Request, Response, Server};
use std::{io::Read, path::Path, sync::Arc};

/// The address Wardian listens on, taken before the server is built so a busy port is known at
/// once (ADR-2610080930).
pub struct Listener(Server);

/// Takes `addr`. Port 0 lets the system pick a free port.
pub fn listen(addr: &str) -> std::io::Result<Listener> {
    Server::bind(addr).map(Listener)
}

impl Listener {
    /// The address actually taken, with the system's port when port 0 was asked for.
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.0.local_addr()
    }
}

/// Answers every connection on `listener` in its own thread. The admin token (ADMIN_TOKEN, or the
/// one saved in Settings, read from `services.keys` on each request) lets other machines change
/// settings; without it, only a browser on this machine may. Returns why it stopped: the system
/// stopped accepting connections.
pub fn serve(listener: Listener, services: Services) -> String {
    let addr = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
    let e = listener.0.run(Arc::new(move |req: &mut Request<'_>| {
        let token = services.keys.admin_token();
        handle(req, &services, token.as_deref())
    }));
    format!("stopped accepting connections on {addr}: {e}")
}

const INDEX_HTML: &str = include_str!("../../../static/index.html");
const KERNEL_HTML: &str = include_str!("../../../static/kernel.html");
const LOGO_SVG: &str = include_str!("../../../static/logo.svg");
const CHANNELS_JS: &str = include_str!("../../../static/channels.js");
const STATE_JS: &str = include_str!("../../../static/state.js");
const SNAPSHOT_JS: &str = include_str!("../../../static/snapshot.js");
/// Channels for page apps, and the standard components (the same ones suite frames get).
const SDK_JS: &str = concat!(include_str!("../../../static/sdk.js"), "\n", include_str!("../../../static/ui/progress.js"));
const MAX_BODY_BYTES: u64 = 64 * 1024;

fn header(k: &str, v: &str) -> Header {
    Header::new(k, v)
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
/// replies, even when opened in its own tab. Its policy also lets it request
/// only its own package and the page library (ADR-2610081003), so it cannot
/// send what it holds anywhere. The CORS header lets such a sandboxed page
/// still load its own scripts and .wasm files from here.
fn app_file(bytes: Vec<u8>, rel: &str, policy: &str) -> Response {
    let ct = mime(Path::new(rel));
    let mut resp = Response::from_data(bytes)
        .with_header(header("Content-Type", ct))
        .with_header(header("Cache-Control", "no-cache"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Access-Control-Allow-Origin", "*"));
    if ct.starts_with("text/html") || ct == "image/svg+xml" {
        resp = resp.with_header(header("Content-Security-Policy", policy));
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

/// The answer to "Save as web page" (ADR-2610080905), added to every page app's HTML: a page is
/// sandboxed, so only its own script can copy what it shows. Appended at the end, so the page's
/// doctype and head stay as they are; it only listens for a message from Wardian's page.
const PAGE_SNAPSHOT: &str = concat!(
    "\n<script>(() => {\n",
    include_str!("../../../static/snapshot.js"),
    include_str!("../../../static/page-snapshot.js"),
    "})();</script>\n"
);

fn with_snapshot(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = bytes;
    out.extend_from_slice(PAGE_SNAPSHOT.as_bytes());
    out
}

fn req_header<'a>(req: &'a Request<'_>, name: &str) -> Option<&'a str> {
    req.header(name)
}

fn query<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split_once('?')?.1.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key && !v.is_empty()).then_some(v)
    })
}

/// Whether a guess is the token, in a time that does not depend on how much of the guess is right,
/// or on whether its length is (ADR-2610081041): both are hashed, and the fixed-size digests are
/// compared byte by byte without stopping early.
fn same_bytes(guess: &str, token: &str) -> bool {
    use ring::digest::{digest, SHA256};
    let (a, b) = (digest(&SHA256, guess.as_bytes()), digest(&SHA256, token.as_bytes()));
    a.as_ref().iter().zip(b.as_ref()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The host a Host header names, without its port; None when it is not `host[:port]` or
/// `[v6]:port`, so that `[::1].evil.com` is not read as `::1`.
fn host_name(header: &str) -> Option<&str> {
    let (host, port) = match header.strip_prefix('[') {
        Some(rest) => {
            let (host, after) = rest.split_once(']')?;
            (host, if after.is_empty() { None } else { Some(after.strip_prefix(':')?) })
        }
        None => match header.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (header, None),
        },
    };
    port.is_none_or(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())).then_some(host)
}

/// Who may change settings and browse Drive. With ADMIN_TOKEN set, whoever
/// sends it in `X-Admin-Token`. Without it, only a browser on this machine:
/// the peer must be loopback, and the Host header must name loopback too, so
/// a hostile web page cannot reach the API through DNS rebinding.
fn is_admin(req: &Request<'_>, token: Option<&str>) -> bool {
    if let Some(token) = token {
        return req_header(req, "X-Admin-Token").is_some_and(|t| same_bytes(t, token));
    }
    let peer_local = req.remote_addr().is_some_and(|a| a.ip().is_loopback());
    let host = host_name(req_header(req, "Host").unwrap_or(""));
    peer_local && matches!(host, Some("localhost" | "127.0.0.1" | "::1"))
}

/// A docs page, or 404 for a name the site does not have.
fn docs_page(html: Option<String>) -> Response {
    match html {
        Some(html) => Response::from_string(html)
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-cache")),
        None => Response::from_string("not found").with_status_code(404),
    }
}

fn json_resp(code: u16, v: Value) -> Response {
    Response::from_data(v.to_string().into_bytes())
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn result_resp(r: Result<Value, String>) -> Response {
    match r {
        Ok(v) => json_resp(200, v),
        Err(e) => json_resp(400, json!({ "error": e })),
    }
}

/// Reads a JSON body. Requiring the JSON content type also blocks plain HTML
/// form posts from other sites, since browsers must ask first (CORS
/// preflight) before sending JSON, and this server never says yes.
fn read_json(req: &mut Request<'_>) -> Result<Value, String> {
    read_json_upto(req, MAX_BODY_BYTES)
}

/// The viewer's state can be larger than a setting: an app may keep up to 1 MB (ADR-2610071055).
const MAX_STATE_BODY_BYTES: u64 = 2 * 1024 * 1024;

fn read_json_upto(req: &mut Request<'_>, max: u64) -> Result<Value, String> {
    let ct = req_header(req, "Content-Type").unwrap_or("");
    if !ct.starts_with("application/json") {
        return Err("expected Content-Type: application/json".into());
    }
    let mut body = String::new();
    req.as_reader()
        .take(max + 1)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > max {
        return Err("body too large".into());
    }
    serde_json::from_str(&body).map_err(|e| format!("invalid JSON: {e}"))
}

/// Reads an uploaded zip. Like the JSON check above, the zip content type is
/// one a browser will not send across sites without asking first.
fn read_zip(req: &mut Request<'_>) -> Result<Vec<u8>, String> {
    let ct = req_header(req, "Content-Type").map(String::from);
    read_zip_body(ct.as_deref(), req.as_reader())
}

/// The zip in `body`, if `content_type` says it is one and it is at most 100 MB.
fn read_zip_body(content_type: Option<&str>, body: &mut dyn Read) -> Result<Vec<u8>, String> {
    if content_type != Some("application/zip") {
        return Err("expected Content-Type: application/zip".into());
    }
    let mut zip = Vec::new();
    body.take(MAX_ZIP_BYTES + 1).read_to_end(&mut zip).map_err(|e| e.to_string())?;
    if zip.len() as u64 > MAX_ZIP_BYTES {
        return Err("the zip is larger than 100 MB".into());
    }
    Ok(zip)
}

/// A long call asked to run as a background job (ADR-2610072118): it answers {job: id} at once,
/// after the same permission checks as a call that waits.
fn background(body: &Value) -> bool {
    body["background"].as_bool() == Some(true)
}

/// The routes whose calls can take minutes when they are not run as a job.
fn long_route(path: &str) -> bool {
    matches!(path, "/api/splunk/search" | "/api/ai/sample" | "/api/db/search-into")
}

/// A long call that waited (no `background`) lets its connection go after it answers, so the
/// browser does not keep it held (ADR-2610072118, decision 5).
fn let_go_after(path: &str, in_background: bool, resp: Response) -> Response {
    if long_route(path) && !in_background { resp.closing() } else { resp }
}

/// Background jobs (ADR-2610072118): GET /api/jobs, GET /api/jobs/<id>, POST /api/jobs/<id>/cancel.
/// The kernel names its package (`?package=` or in the body), and sees only that package's jobs;
/// without a package, the admin's own page sees them all.
fn jobs_route(method: &Method, rest: &[&str], package: Option<&str>, s: &Services) -> Result<Value, String> {
    if package.is_some_and(|p| !safe_segment(p)) {
        return Err("not an app name".into());
    }
    match (method, rest) {
        (Method::Get, []) => Ok(s.jobs.list(package)),
        (Method::Get, [id]) => s.jobs.get(id, package),
        (Method::Post, [id, "cancel"]) => s.jobs.cancel(id, package),
        _ => Err("not found".into()),
    }
}

fn api_post(path: &str, body: Value, s: &Services) -> Result<Value, String> {
    let (hub, studio, splunk) = (&s.catalog, &s.builder, &s.searches);
    // The key list (ADR-2610081500): /api/keys/<id>/test and /api/keys/<id>/remove.
    if let Some(rest) = path.strip_prefix("/api/keys/") {
        return match rest.split_once('/') {
            Some((id, "test")) => s.keys.test(id),
            Some((id, "remove")) => s.keys.remove(id),
            None if rest == "admin" => s.keys.set_admin_token(&body),
            _ => Err("not found".into()),
        };
    }
    match path {
        "/api/agent" => studio.set_agent(&body),
        "/api/splunk/config" => splunk.set_config(&body),
        "/api/splunk/search" => {
            // The kernel asks for an app of a suite. Check here too, not only in the
            // browser: the app must declare "splunk" and the user must have allowed it.
            let package = body["package"].as_str().unwrap_or("");
            let app = body["app"].as_str().unwrap_or("");
            hub.check_host_cap(package, app, "splunk", "splunk")?;
            if background(&body) {
                return s.jobs.start(JobKind::SplunkSearch, package, app, &body);
            }
            let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
            let out = splunk.search(&s("search"), &s("earliest"), &s("latest"), &Unwatched);
            println!("splunk: {package}/{app} ran a search: {}", if out.is_ok() { "ok" } else { "failed" });
            out
        }
        "/api/grants" => hub.set_grant(&body),
        "/api/setup/done" => hub.setup_done(),
        "/api/ai/key" => studio.set_key(body["key"].as_str().unwrap_or(""), body["workspace"].as_str()),
        "/api/ai/provider" => studio.set_provider(&body),
        "/api/ai/send" => studio.send(&body),
        "/api/ai/sample" => {
            // claude:sample for a suite app, billed to the saved Anthropic key. Same checks as splunk.
            let package = body["package"].as_str().unwrap_or("");
            let app = body["app"].as_str().unwrap_or("");
            hub.check_host_cap(package, app, "claude:sample", "ai").map_err(|e| format!("not_granted: {e}"))?;
            if background(&body) {
                return s.jobs.start(JobKind::AiSample, package, app, &body);
            }
            let out = studio.sample(&body);
            println!("ai: {package}/{app} asked Claude: {}", match &out { Ok(_) => "ok".to_string(), Err(e) => e.clone() });
            out
        }
        "/api/ai/stop" => studio.stop(body["session"].as_str().unwrap_or("")),
        "/api/ai/claim-test" => {
            let saved = body["saved"].as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(usize::MAX);
            studio.claim_test(body["session"].as_str().unwrap_or(""), saved)
        }
        "/api/ai/tested" => studio.tested(body["session"].as_str().unwrap_or(""), &body),
        "/api/refresh" => {
            hub.refresh()?;
            Ok(hub.status())
        }
        "/api/drive/key" => {
            // The body is the key file exactly as Google issued it.
            let email = hub.set_drive_key(&body.to_string())?;
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

/// A change to the viewer's state: /api/state/layout/<package>, /api/state/apps/<package> or
/// /api/state/channel/<name>.
fn post_state(kind: &str, name: &str, body: Value, s: &Services) -> Result<Value, String> {
    let st = &s.state;
    match kind {
        "layout" => st.set_layout(name, body["layout"].clone()),
        "apps" if body.get("merge").is_some() => st.merge_app_data(name, &body["merge"]),
        "apps" => st.set_app_value(name, body["app"].as_str().unwrap_or(""), body["key"].as_str().unwrap_or(""), body["value"].clone()),
        "channel" => st.set_channel(name, body["message"].clone()),
        _ => Err("not found".into()),
    }
}

/// The `db` capability (ADR-2610071219). The app must declare "db" in suite.json; reading another
/// package's tables also needs the user's permission ("tables.<package>"), and loading a Splunk
/// search needs "splunk" and its permission.
fn post_db(op: &str, body: Value, s: &Services) -> Result<Value, String> {
    let package = body["package"].as_str().unwrap_or("");
    let app = body["app"].as_str().unwrap_or("");
    s.catalog.check_host_cap(package, app, "db", "")?;
    match op {
        "query" => s.tables.query(package, body["sql"].as_str().unwrap_or(""), &body["params"]),
        "page" => {
            let source = body["source"].as_str().filter(|x| !x.is_empty() && *x != package);
            if let Some(src) = source {
                s.catalog.check_host_cap(package, app, "db", &format!("tables.{src}"))?;
            }
            s.tables.page(package, source, &body)
        }
        "insert" => s.tables.insert(package, &body),
        "tables" => s.tables.tables(package),
        "search-into" => {
            s.catalog.check_host_cap(package, app, "splunk", "splunk")?;
            if background(&body) {
                return s.jobs.start(JobKind::SplunkInto, package, app, &body);
            }
            let t = |k: &str| body[k].as_str().unwrap_or("").to_string();
            let table = Some(t("table")).filter(|x| !x.is_empty()).unwrap_or_else(|| "search".into());
            let out = s.searches.search_into(package, &table, &t("search"), &t("earliest"), &t("latest"), &Unwatched);
            match &out {
                Ok(v) => println!("splunk: {package}/{app} loaded {} rows into {table}", v["total"]),
                Err(_) => println!("splunk: {package}/{app} loading a search failed"),
            }
            out
        }
        _ => Err("not found".into()),
    }
}

fn handle(req: &mut Request<'_>, s: &Services, token: Option<&str>) -> Response {
    let (hub, studio, splunk) = (&s.catalog, &s.builder, &s.searches);
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let admin = is_admin(req, token);

    match (req.method().clone(), parts.as_slice()) {
        (Method::Post, ["api", "import"]) if admin => result_resp(read_zip(req).and_then(|bytes| {
            let name = query(&url, "name").unwrap_or("imported.zip");
            let replace = query(&url, "replace") == Some("1");
            // A .wardian file's data is installed only when asked (ADR-2610071248).
            if query(&url, "data") != Some("1") {
                return hub.import(&bytes, name, replace);
            }
            let package = s.exports.preview_import(&bytes)?["package"].as_str().map(String::from);
            if let Some(p) = &package {
                if replace {
                    s.exports.keep_data_before_import(p)?;
                }
            }
            let mut out = hub.import(&bytes, name, replace)?;
            if let Some(p) = package.filter(|p| out["apps"].as_array().is_some_and(|a| a.iter().any(|x| x == p.as_str()))) {
                out["data"] = s.exports.install_data(&p, &bytes)?;
            }
            Ok(out)
        })),
        // What a .wardian file holds, before anything is installed.
        (Method::Post, ["api", "import", "preview"]) if admin => result_resp(read_zip(req).and_then(|bytes| s.exports.preview_import(&bytes))),
        // An app as a .wardian file; `preview=1` lists what it would hold without building it.
        (Method::Get, ["api", "apps", app, "export"]) if admin => {
            let with_data = query(&url, "data") == Some("1");
            if query(&url, "preview") == Some("1") {
                result_resp(s.exports.preview(app, with_data))
            } else {
                match s.exports.export(app, with_data) {
                    Ok(bytes) => Response::from_data(bytes)
                        .with_header(header("Content-Type", EXPORT_MIME))
                        .with_header(header("Content-Disposition", &format!("attachment; filename=\"{}\"", export_file_name(app))))
                        .with_header(header("Cache-Control", "no-store")),
                    Err(e) => json_resp(400, json!({ "error": e })),
                }
            }
        }
        // The viewer's state, kept by the host. Like the settings it needs admin; without it the
        // page keeps the state in the browser instead.
        (Method::Post, ["api", "db", op]) if admin => match read_json_upto(req, MAX_STATE_BODY_BYTES) {
            Ok(body) => let_go_after(path, background(&body), result_resp(post_db(op, body, s))),
            Err(e) => json_resp(400, json!({ "error": e })),
        },
        (Method::Get, ["api", "jobs", rest @ ..]) if admin => result_resp(jobs_route(&Method::Get, rest, query(&url, "package"), s)),
        (Method::Post, ["api", "jobs", rest @ ..]) if admin => result_resp(read_json(req).and_then(|body| {
            let package = body["package"].as_str().filter(|p| !p.is_empty()).map(String::from);
            jobs_route(&Method::Post, rest, package.as_deref(), s)
        })),
        // The folders of the app list (ADR-2610081830), tidied against the apps being served.
        (Method::Get, ["api", "state", "folders"]) if admin => result_resp(s.state.folders(&hub.list_apps())),
        (Method::Post, ["api", "state", "folders"]) if admin => {
            result_resp(read_json(req).and_then(|body| s.state.set_folders(&body, &hub.list_apps())))
        }
        (Method::Post, ["api", "state", kind, name]) if admin => {
            result_resp(read_json_upto(req, MAX_STATE_BODY_BYTES).and_then(|body| post_state(kind, name, body, s)))
        }
        // Each local app's history (ADR-2610071122): versions, what changed, restore.
        (Method::Get, ["api", "history", app]) if admin => result_resp(s.history.versions(app)),
        (Method::Get, ["api", "history", app, n, "diff"]) if admin => match n.parse::<u64>() {
            Ok(n) => result_resp(s.history.diff(app, n)),
            Err(_) => json_resp(400, json!({ "error": "not a version number" })),
        },
        (Method::Post, ["api", "history", app, "restore"]) if admin => result_resp(read_json(req).and_then(|body| {
            let n = body["n"].as_u64().ok_or("which version? send {\"n\": <number>}")?;
            s.history.restore(app, n)
        })),
        (Method::Get, ["api", "state", "layout", name]) if admin => result_resp(s.state.layout(name)),
        (Method::Get, ["api", "state", "apps", name]) if admin => result_resp(s.state.app_data(name)),
        (Method::Get, ["api", "state", "channel", name]) if admin => json_resp(200, json!({ "message": s.state.channel(name) })),
        (Method::Post, ["api", ..]) => {
            if !admin {
                json_resp(403, json!({ "error": "settings are locked; see ADMIN_TOKEN in the README" }))
            } else {
                match read_json(req) {
                    Ok(body) => let_go_after(path, background(&body), result_resp(api_post(path, body, s))),
                    Err(e) => json_resp(400, json!({ "error": e })),
                }
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
        // The path arrives without its slashes, so /docs and /docs/ are both the home page.
        (Method::Get, ["docs"]) => docs_page(s.pages.page("index")),
        (Method::Get, ["docs", name]) => docs_page(s.pages.page(name)),
        (Method::Get, ["docs", parent, name]) => docs_page(s.pages.page(&format!("{parent}/{name}"))),
        (Method::Get, ["schemas", file]) => match s.pages.schema(file) {
            Some(body) => {
                Response::from_string(body)
                    .with_header(header("Content-Type", "application/schema+json"))
                    .with_header(header("Access-Control-Allow-Origin", "*"))
                    .with_header(header("Cache-Control", "no-cache"))
            }
            None => Response::from_string("not found").with_status_code(404),
        },
        // Browsers ask for a tab icon on every page; "no content" keeps 404s out of the console.
        (Method::Get, ["favicon.ico"]) => Response::from_data(Vec::new()).with_status_code(204),
        // channels.js runs in Wardian's own pages; sdk/wardian.js is for page apps, which have a
        // throwaway origin, so it carries the CORS header like app files. sdk/rustle.js is its
        // name from before the rename.
        (Method::Get, ["channels.js"]) => Response::from_string(CHANNELS_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Cache-Control", "no-cache")),
        // The viewer's state, kept by the server: Wardian's own pages load it (ADR-2610071055).
        // The default rendering for Save as web page (ADR-2610080905): Wardian's page copies a module
        // app's cards with it.
        (Method::Get, ["snapshot.js"]) => Response::from_string(SNAPSHOT_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["state.js"]) => Response::from_string(STATE_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["sdk", "wardian.js" | "rustle.js"]) => Response::from_string(SDK_JS)
            .with_header(header("Content-Type", "text/javascript"))
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header("Cache-Control", "no-cache")),
        // The component library: the gallery, and each file, for `wardian add` users to read.
        (Method::Get, ["ui"]) => Response::from_string(s.pages.gallery())
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-cache")),
        (Method::Get, ["ui", f]) => match s.pages.ui_file(f) {
            Some(body) => Response::from_string(body)
                .with_header(header("Content-Type", mime(Path::new(f))))
                .with_header(header("Cache-Control", "no-cache")),
            None => Response::from_string("not found").with_status_code(404),
        },
        (Method::Get, ["api", "grants"]) => json_resp(200, hub.grants()),
        (Method::Get, ["logo.svg"]) => Response::from_string(LOGO_SVG)
            .with_header(header("Content-Type", "image/svg+xml"))
            .with_header(header("Cache-Control", "max-age=86400")),
        (Method::Get, ["api", "apps"]) => json_resp(200, json!(hub.list_apps())),
        (Method::Get, ["api", "app-list"]) => json_resp(200, json!(hub.apps())),
        (Method::Get, ["api", "status"]) => {
            let mut s = hub.status();
            s["admin"] = json!(admin);
            s["ai"] = studio.status();
            s["splunk"] = splunk.status();
            // The Anthropic workspace ID is a setting only an admin sees and edits.
            if let Some(ai) = s["ai"].as_object_mut().filter(|_| !admin) {
                ai.remove("workspace");
            }
            json_resp(200, s)
        }
        (Method::Get, ["api", "trash"]) if admin => json_resp(200, hub.trash()),
        // Every secret, Claude's settings and the tokens used (ADR-2610081500). Admin only: they
        // name accounts, models and apps, never a secret.
        (Method::Get, ["api", "keys"]) if admin => json_resp(200, s.keys.list()),
        (Method::Get, ["api", "agent"]) if admin => json_resp(200, studio.agent()),
        (Method::Get, ["api", "ai", "aws-profiles"]) if admin => json_resp(200, studio.aws_profiles()),
        (Method::Get, ["api", "ai", "aws-profiles"]) => json_resp(403, json!({ "error": "settings are locked" })),
        (Method::Get, ["api", "usage"]) if admin => json_resp(200, studio.usage()),
        (Method::Get, ["api", "keys" | "agent" | "usage"]) => json_resp(403, json!({ "error": "settings are locked" })),
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
            match hub.frame(name, app) {
                Ok(html) => Response::from_string(html)
                    .with_header(header("Content-Type", "text/html; charset=utf-8"))
                    .with_header(header("Content-Security-Policy", FRAME_CSP))
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
            let bytes = if safe_rel(&rel) { hub.read(name, &rel) } else { None };
            let policy = page_csp(req_header(req, "Host").unwrap_or(""), name);
            match bytes {
                Some(bytes) if query(&url, "wardian-probe") == Some("1") && rel.ends_with(".html") => app_file(with_snapshot(with_probe(bytes)), &rel, &policy),
                Some(bytes) if rel.ends_with(".html") => app_file(with_snapshot(bytes), &rel, &policy),
                Some(bytes) => app_file(bytes, &rel, &policy),
                None => Response::from_string("not found").with_status_code(404),
            }
        }
        (Method::Get, _) => Response::from_string("not found").with_status_code(404),
        _ => Response::from_string("method not allowed").with_status_code(405),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_gets_the_snapshot_answer_at_its_end() {
        let page = b"<!doctype html>\n<title>x</title><p>hi</p>".to_vec();
        let out = String::from_utf8(with_snapshot(page)).unwrap();
        assert!(out.starts_with("<!doctype html>"), "the doctype stays first");
        assert!(out.contains("wardian: 'snapshot'") && out.contains("WardianSnapshot"));
        // One script, closed once: nothing inside it may end it early.
        assert_eq!(PAGE_SNAPSHOT.matches("</script").count(), 1);
        assert!(out.trim_end().ends_with("})();</script>"));
    }

    /// security.md Admin, ADR-2610081041 (#136, C9): the token check accepts only the token itself;
    /// a guess of another length, or of the same length, is refused.
    #[test]
    fn claim_the_admin_token_check_accepts_only_the_token() {
        let token = "s3cret-token-0123456789";
        assert!(same_bytes(token, token));
        for guess in ["", "s", "s3cret-token", "s3cret-token-012345678", "s3cret-token-0123456780", "S3cret-token-0123456789", "s3cret-token-01234567890", "s3cret-token-0123456789\0"] {
            assert!(!same_bytes(guess, token), "{guess:?}");
        }
    }

    /// security.md, SPEC 3.5 (#145): an uploaded zip of more than 100 MB is refused, and only a zip
    /// Content-Type is read.
    #[test]
    fn claim_an_upload_over_100_mb_is_refused() {
        let mut big = std::io::repeat(b'x').take(MAX_ZIP_BYTES + 1);
        assert_eq!(read_zip_body(Some("application/zip"), &mut big).unwrap_err(), "the zip is larger than 100 MB");
        assert_eq!(read_zip_body(Some("application/zip"), &mut &b"PK"[..]).unwrap(), b"PK");
        for ct in [None, Some("text/plain"), Some("multipart/form-data")] {
            assert!(read_zip_body(ct, &mut &b"PK"[..]).unwrap_err().contains("expected Content-Type: application/zip"), "{ct:?}");
        }
    }
}
