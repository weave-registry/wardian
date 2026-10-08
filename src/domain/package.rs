//! The package model: what a package is, which names and paths it may use, and what the app
//! list shows for it. Pure: it reads a package through the closures it is given.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// How deep an app's own folders may go, and how many files it may have.
pub const MAX_DEPTH: usize = 8;
pub const MAX_APP_FILES: usize = 2_000;
/// The newest package format this host understands (SPEC.md). A package
/// without a "format" field is format 1.
pub const FORMAT: u64 = 2;
/// Folders never served or copied: build caches and package downloads.
pub const SKIP_DIRS: &[&str] = &["node_modules", "target"];
/// Where to look for an app's page when app.json does not name one.
pub const PAGE_GUESSES: &[&str] = &["index.html", "demo/index.html", "www/index.html", "web/index.html"];
/// A folder is an app if it holds a module (app.wasm) or a suite of
/// cooperating apps (suite.json).
pub const APP_MARKERS: &[&str] = &["app.wasm", "suite.json"];

/// Accept only simple names: no slashes, no "..", no hidden files.
pub fn safe_segment(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('.') && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Drive IDs are opaque but always URL-safe; anything else is rejected so an
/// ID can never break out of the quoted Drive query it is placed in.
pub fn valid_drive_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A path inside an app, like "pkg/usl.js": every part a safe name.
pub fn safe_rel(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    parts.len() <= MAX_DEPTH + 1 && parts.iter().all(|p| safe_segment(p))
}

/// A path the host may serve from an app (SPEC.md 3.3, 3.4): a safe path with no folder named like
/// a build folder, in any case, since a disk that ignores case finds `Target/` as `target/`.
pub fn servable_rel(rel: &str) -> bool {
    safe_rel(rel) && !rel.split('/').any(|p| SKIP_DIRS.iter().any(|d| p.eq_ignore_ascii_case(d)))
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The file ID in a Drive link: .../file/d/<ID>/view or ...?id=<ID>.
pub fn drive_file_id(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let (host, path) = rest.split_once('/')?;
    if host != "drive.google.com" && host != "docs.google.com" {
        return None;
    }
    let id = match path.split_once("file/d/") {
        Some((_, after)) => after.split(['/', '?', '#']).next()?,
        None => path.split(['?', '&']).find_map(|kv| kv.strip_prefix("id="))?,
    };
    valid_drive_id(id).then(|| id.to_string())
}

/// The optional app.json at the top of an app.
#[derive(Deserialize, Default)]
struct Manifest {
    format: Option<u64>,
    title: Option<String>,
    description: Option<String>,
    page: Option<String>,
    /// Read loosely (as JSON values): a wrong type here must not hide the title.
    channels: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct SuiteHead {
    format: Option<u64>,
    title: Option<String>,
    description: Option<String>,
    apps: Option<serde_json::Value>,
}

/// What a package may do outside its own sealed frame. The app page shows it as the "Sealed" label.
#[derive(Serialize, Default)]
pub struct Allows {
    /// Capabilities from suite.json, such as "storage" or "claude:downloads".
    pub caps: Vec<String>,
    /// Channels it may send on and read from, after the user agrees.
    pub send: Vec<String>,
    pub receive: Vec<String>,
}

/// What the app list shows for each app.
#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The app's own page, relative to the app folder, if it has one.
    pub page: Option<String>,
    /// True for a suite: several apps run together by the host kernel.
    pub suite: bool,
    /// Why the host cannot run this app, e.g. it needs a newer format.
    pub error: Option<String>,
    pub allows: Allows,
}

/// The strings in a JSON array; anything else counts as empty.
fn strings(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// What the app list shows for app `name`, read through `read` and `has` (paths inside the app).
pub fn app_info(name: String, read: &dyn Fn(&str) -> Option<Vec<u8>>, has: &dyn Fn(&str) -> bool) -> AppInfo {
    let m: Manifest = read("app.json").and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let named = m.page.filter(|p| safe_rel(p) && has(p));
    let page = named.or_else(|| PAGE_GUESSES.iter().find(|p| has(p)).map(|p| p.to_string()));
    let suite = has("suite.json");
    let head: SuiteHead = if suite { read("suite.json").and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() } else { SuiteHead::default() };
    let needed = m.format.into_iter().chain(head.format).max().unwrap_or(1);
    let error = (needed > FORMAT).then(|| format!("needs package format {needed}; this Wardian reads format {FORMAT}. Update Wardian."));
    // app.json wins; a suite's own title and description fill the gaps.
    let title = m.title.or(head.title);
    let description = m.description.or(head.description);
    // A suite may do what any of its apps may do.
    let mut allows = Allows::default();
    let entries = head.apps.as_ref().and_then(|a| a.as_array()).cloned().unwrap_or_default();
    for ch in m.channels.iter().chain(entries.iter().filter_map(|e| e.get("channels"))) {
        allows.send.extend(strings(ch.get("send")));
        allows.receive.extend(strings(ch.get("receive")));
    }
    for e in &entries {
        allows.caps.extend(strings(e.get("caps")));
    }
    for v in [&mut allows.caps, &mut allows.send, &mut allows.receive] {
        v.sort();
        v.dedup();
    }
    AppInfo { name, title, description, page, suite, error, allows }
}

/// How fresh a served Google Drive folder is.
#[derive(Default, Clone, Serialize)]
pub struct RefreshStatus {
    /// Unix time of the last successful index.
    pub last_ok: Option<u64>,
    /// The error from the last attempt, cleared by a success.
    pub last_error: Option<String>,
}

/// A folder (or shared drive) shown in the Drive folder browser.
#[derive(Serialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub shared_drive: bool,
}

/// Which apps the host serves, saved as `<data dir>/config.json`.
#[derive(Default, Serialize, Deserialize)]
pub struct SourceChoice {
    /// "local" or "drive"
    pub source: String,
    pub folder_id: Option<String>,
    pub folder_name: Option<String>,
}

/// The removed apps' trash ids are "<name>--<unix time>", with '_' added when two collide.
pub fn trash_entry(id: &str) -> Option<(u64, String)> {
    let (name, when) = id.rsplit_once("--")?;
    Some((when.trim_end_matches('_').parse().ok()?, name.to_string()))
}

/// A random number, for names that only need to differ (staging folders, chat ids). It comes
/// from the standard library's per-process random hash keys, so it needs no system call.
pub fn random_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    u32::try_from(h.finish() & 0xffff_ffff).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SPEC 3.3 (#161): a name matches `[A-Za-z0-9_-][A-Za-z0-9_.-]*`, and a path inside a package
    /// is made of such names, at most 8 folders deep.
    #[test]
    fn claim_names_match_the_pattern() {
        for ok in ["a", "A-b_c.d", "_x", "-x", "9", "app.wasm", "x.."] {
            assert!(safe_segment(ok), "{ok}");
        }
        for bad in ["", ".", "..", ".env", ".trash", "a b", "a/b", "a\\b", "a%20b", "é", "a:b", "a\0b", "a?b"] {
            assert!(!safe_segment(bad), "{bad:?}");
        }
        assert!(safe_rel("a/b/c.js") && safe_rel(&["d"; MAX_DEPTH + 1].join("/")));
        assert!(!safe_rel(&["d"; MAX_DEPTH + 2].join("/")), "deeper than 8 folders");
        for bad in ["", "a//b", "/a", "a/", "a/../b", "a/.git/x"] {
            assert!(!safe_rel(bad), "{bad:?}");
        }
    }

    /// SPEC 3.4 (#162): nothing inside a folder named `node_modules` or `target` is served, in any case.
    #[test]
    fn claim_build_folders_are_never_servable() {
        for ok in ["index.html", "pkg/app.js", "targets/a", "my_target/a", "node_modules.txt"] {
            assert!(servable_rel(ok), "{ok}");
        }
        for never in ["target/a", "node_modules/a.js", "sub/target/a", "a/b/node_modules/c.js", "TARGET/a", "Node_Modules/a.js", ".env", "a/.git/x"] {
            assert!(!servable_rel(never), "{never}");
        }
    }

    /// SPEC 5.3 (#166): the page is app.json's `page`, or else the first of the guesses that exists.
    #[test]
    fn claim_the_page_is_named_or_guessed() {
        let info = |manifest: &str, files: &[&str]| {
            let manifest = manifest.as_bytes().to_vec();
            let has = |rel: &str| files.contains(&rel);
            app_info("x".into(), &|rel| (rel == "app.json").then(|| manifest.clone()), &has).page
        };
        assert_eq!(PAGE_GUESSES, ["index.html", "demo/index.html", "www/index.html", "web/index.html"]);
        for (i, guess) in PAGE_GUESSES.iter().enumerate() {
            assert_eq!(info("{}", &PAGE_GUESSES[i..]).as_deref(), Some(*guess), "the first that exists wins");
        }
        assert_eq!(info("{}", &["other/index.html"]), None);
        assert_eq!(info(r#"{"page": "ui/main.html"}"#, &["ui/main.html", "index.html"]).as_deref(), Some("ui/main.html"));
        assert_eq!(info(r#"{"page": "missing.html"}"#, &["www/index.html"]).as_deref(), Some("www/index.html"), "a named page that is missing falls back");
        assert_eq!(info(r#"{"page": "../x.html"}"#, &["../x.html"]), None, "a named page must be a safe path");
    }
}
