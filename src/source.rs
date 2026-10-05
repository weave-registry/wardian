//! Where apps come from: a local directory, or a Google Drive folder
//! accessed directly through the Drive API with a service account.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// How deep an app's own folders may go, and how many files it may have.
pub const MAX_DEPTH: usize = 8;
pub const MAX_APP_FILES: usize = 2_000;
/// The newest package format this host understands (SPEC.md). A package
/// without a "format" field is format 1.
pub const FORMAT: u64 = 1;
/// Folders never served or copied: build caches and package downloads.
pub const SKIP_DIRS: &[&str] = &["node_modules", "target"];
/// Where to look for an app's page when app.json does not name one.
const PAGE_GUESSES: &[&str] = &["index.html", "demo/index.html", "www/index.html", "web/index.html"];
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

/// Accept only simple names: no slashes, no "..", no hidden files.
pub fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
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

/// The optional app.json at the top of an app.
#[derive(Deserialize, Default)]
struct Manifest {
    format: Option<u64>,
    title: Option<String>,
    description: Option<String>,
    page: Option<String>,
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
}

#[derive(Deserialize, Default)]
struct FormatOnly {
    format: Option<u64>,
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A folder is an app if it holds a module (app.wasm) or a suite of
/// cooperating apps (suite.json).
pub const APP_MARKERS: &[&str] = &["app.wasm", "suite.json"];

fn is_app_dir(p: &std::path::Path) -> bool {
    APP_MARKERS.iter().any(|m| p.join(m).is_file())
}

pub enum Source {
    Local(PathBuf),
    Drive(Arc<Drive>),
}

impl Source {
    pub fn list_apps(&self) -> Vec<String> {
        match self {
            Source::Local(root) => {
                let mut names: Vec<String> = fs::read_dir(root)
                    .map(|rd| {
                        rd.flatten()
                            .filter(|e| is_app_dir(&e.path()))
                            .filter_map(|e| e.file_name().into_string().ok())
                            .filter(|n| safe_segment(n))
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                names
            }
            Source::Drive(d) => d.list_apps(),
        }
    }

    /// `rel` must already have passed `safe_rel`.
    pub fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>> {
        match self {
            Source::Local(root) => fs::read(root.join(app).join(rel)).ok(),
            Source::Drive(d) => d.read(app, rel),
        }
    }

    fn has(&self, app: &str, rel: &str) -> bool {
        match self {
            Source::Local(root) => root.join(app).join(rel).is_file(),
            Source::Drive(d) => d.has(app, rel),
        }
    }

    pub fn apps(&self) -> Vec<AppInfo> {
        self.list_apps().into_iter().map(|name| self.info(name)).collect()
    }

    fn info(&self, name: String) -> AppInfo {
        let m: Manifest = self
            .read(&name, "app.json")
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let named = m.page.filter(|p| safe_rel(p) && self.has(&name, p));
        let page = named.or_else(|| {
            PAGE_GUESSES.iter().find(|p| self.has(&name, p)).map(|p| p.to_string())
        });
        let suite = self.has(&name, "suite.json");
        let suite_format = if suite {
            self.read(&name, "suite.json").and_then(|b| serde_json::from_slice::<FormatOnly>(&b).ok()).unwrap_or_default().format
        } else {
            None
        };
        let needed = m.format.into_iter().chain(suite_format).max().unwrap_or(1);
        let error = (needed > FORMAT).then(|| format!("needs package format {needed}; this rustle reads format {FORMAT}. Update rustle."));
        AppInfo { name, title: m.title, description: m.description, page, suite, error }
    }
}

/// A short, readable reason for a failed Google API call. ureq's own text
/// repeats the whole request URL; Google's JSON body names the real cause.
fn api_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: serde_json::Value = resp.into_json().unwrap_or_default();
            let msg = body["error"]["message"]
                .as_str()
                .or_else(|| body["error_description"].as_str())
                .or_else(|| body["error"].as_str());
            match msg {
                Some(m) => format!("HTTP {code}: {m}"),
                None => format!("HTTP {code}"),
            }
        }
        ureq::Error::Transport(t) => match t.message() {
            Some(m) => format!("{}: {m}", t.kind()),
            None => t.kind().to_string(),
        },
    }
}

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    md5_checksum: Option<String>, // absent for Google Docs/Sheets, which we skip
    #[serde(default)]
    mime_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct SharedDrive {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct DriveList {
    #[serde(default)]
    drives: Vec<SharedDrive>,
}

/// A folder (or shared drive) shown in the folder browser.
#[derive(Serialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub shared_drive: bool,
}

#[derive(Clone)]
struct FileMeta {
    id: String,
    md5: String,
}

/// app name -> (file name -> Drive file)
type Index = HashMap<String, HashMap<String, FileMeta>>;

/// An authenticated connection to the Drive API. It is not tied to a folder,
/// so the settings page can browse Drive before an apps folder is chosen.
pub struct DriveClient {
    sa: ServiceAccount,
    api_base: String,
    agent: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
}

impl DriveClient {
    pub fn from_json(raw: &str, api_base: &str) -> Result<Arc<DriveClient>, String> {
        let sa: ServiceAccount =
            serde_json::from_str(raw).map_err(|e| format!("not a service account key: {e}"))?;
        EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
            .map_err(|e| format!("bad private key: {e}"))?;
        Ok(Arc::new(DriveClient {
            sa,
            api_base: api_base.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build(),
            token: Mutex::new(None),
        }))
    }

    /// The address a Drive folder must be shared with.
    pub fn client_email(&self) -> &str {
        &self.sa.client_email
    }

    /// Proves the key works by fetching an access token.
    pub fn check(&self) -> Result<(), String> {
        self.token().map(|_| ())
    }

    fn token(&self) -> Result<String, String> {
        let mut guard = self.token.lock().unwrap();
        if let Some((t, exp)) = &*guard {
            if Instant::now() < *exp {
                return Ok(t.clone());
            }
        }
        let now = unix_now();
        let claims = Claims {
            iss: &self.sa.client_email,
            scope: SCOPE,
            aud: &self.sa.token_uri,
            iat: now,
            exp: now + 3600,
        };
        let key = EncodingKey::from_rsa_pem(self.sa.private_key.as_bytes())
            .map_err(|e| format!("bad private key: {e}"))?;
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(|e| e.to_string())?;
        let resp: TokenResp = self
            .agent
            .post(&self.sa.token_uri)
            .send_form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .map_err(|e| format!("Google sign-in failed: {}", api_error(e)))?
            .into_json()
            .map_err(|e| e.to_string())?;
        let exp = Instant::now() + Duration::from_secs(resp.expires_in.saturating_sub(60));
        *guard = Some((resp.access_token.clone(), exp));
        Ok(resp.access_token)
    }

    fn list(&self, q: &str) -> Result<Vec<DriveFile>, String> {
        let tok = self.token()?;
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut req = self
                .agent
                .get(&format!("{}/drive/v3/files", self.api_base))
                .set("Authorization", &format!("Bearer {tok}"))
                .query("q", q)
                .query("fields", "nextPageToken,files(id,name,md5Checksum,mimeType)")
                .query("pageSize", "1000")
                .query("supportsAllDrives", "true")
                .query("includeItemsFromAllDrives", "true");
            if let Some(p) = &page {
                req = req.query("pageToken", p);
            }
            let list: FileList = req
                .call()
                .map_err(|e| format!("Drive list failed: {}", api_error(e)))?
                .into_json()
                .map_err(|e| e.to_string())?;
            out.extend(list.files);
            match list.next_page_token {
                Some(t) => page = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    /// Subfolders of `parent`. With no parent, the top level: folders shared
    /// with the service account plus shared drives it is a member of. (A
    /// service account's own "My Drive" is empty, so that is not listed.)
    pub fn browse(&self, parent: Option<&str>) -> Result<Vec<Folder>, String> {
        let mut out: Vec<Folder> = Vec::new();
        let q = match parent {
            Some(id) if valid_drive_id(id) => {
                format!("'{id}' in parents and mimeType='{FOLDER_MIME}' and trashed=false")
            }
            Some(_) => return Err("invalid folder id".into()),
            None => {
                out.extend(self.shared_drives()?);
                format!("sharedWithMe=true and mimeType='{FOLDER_MIME}' and trashed=false")
            }
        };
        let mut folders: Vec<Folder> = self
            .list(&q)?
            .into_iter()
            .map(|f| Folder { id: f.id, name: f.name, shared_drive: false })
            .collect();
        folders.sort_by_key(|f| f.name.to_lowercase());
        out.extend(folders);
        Ok(out)
    }

    fn shared_drives(&self) -> Result<Vec<Folder>, String> {
        let tok = self.token()?;
        let list: DriveList = self
            .agent
            .get(&format!("{}/drive/v3/drives", self.api_base))
            .set("Authorization", &format!("Bearer {tok}"))
            .query("pageSize", "100")
            .call()
            .map_err(|e| format!("Drive list failed: {}", api_error(e)))?
            .into_json()
            .map_err(|e| e.to_string())?;
        Ok(list
            .drives
            .into_iter()
            .map(|d| Folder { id: d.id, name: d.name, shared_drive: true })
            .collect())
    }

    /// One subfolder per app; a subfolder counts only if it holds app.wasm.
    fn build_index(&self, folder_id: &str) -> Result<Index, String> {
        if !valid_drive_id(folder_id) {
            return Err("folder id looks invalid".into());
        }
        let folders = self.list(&format!(
            "'{folder_id}' in parents and mimeType='{FOLDER_MIME}' and trashed=false"
        ))?;
        let mut new: Index = HashMap::new();
        for f in folders {
            if !safe_segment(&f.name) || new.contains_key(&f.name) {
                continue;
            }
            let mut m: HashMap<String, FileMeta> = HashMap::new();
            self.walk(&f.id, "", 0, &mut m)?;
            if APP_MARKERS.iter().any(|k| m.contains_key(*k)) {
                new.insert(f.name, m);
            }
        }
        Ok(new)
    }

    /// Adds the files under one folder to `m`, keyed by path ("pkg/usl.js").
    /// The top level is listed first, and a folder without app.wasm is not
    /// searched further, so non-app folders cost one API call.
    fn walk(&self, id: &str, prefix: &str, depth: usize, m: &mut HashMap<String, FileMeta>) -> Result<(), String> {
        let items = self.list(&format!("'{id}' in parents and trashed=false"))?;
        if depth == 0 && !items.iter().any(|f| APP_MARKERS.contains(&f.name.as_str()) && f.mime_type != FOLDER_MIME) {
            return Ok(());
        }
        let mut folders = Vec::new();
        for f in items {
            if !safe_segment(&f.name) {
                continue;
            }
            if f.mime_type == FOLDER_MIME {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&f.name.as_str()) {
                    folders.push(f);
                }
            } else if let Some(md5) = f.md5_checksum {
                if m.len() >= MAX_APP_FILES {
                    return Err(format!("an app has more than {MAX_APP_FILES} files"));
                }
                m.entry(format!("{prefix}{}", f.name)).or_insert(FileMeta { id: f.id, md5 });
            }
        }
        for f in folders {
            self.walk(&f.id, &format!("{prefix}{}/", f.name), depth + 1, m)?;
        }
        Ok(())
    }

    /// The app names a folder would serve, without connecting to it.
    pub fn preview(&self, folder_id: &str) -> Result<Vec<String>, String> {
        let mut v: Vec<String> = self.build_index(folder_id)?.into_keys().collect();
        v.sort();
        Ok(v)
    }

    pub fn download(&self, id: &str) -> Result<Vec<u8>, String> {
        let tok = self.token()?;
        let resp = self
            .agent
            .get(&format!("{}/drive/v3/files/{id}", self.api_base))
            .set("Authorization", &format!("Bearer {tok}"))
            .query("alt", "media")
            .query("supportsAllDrives", "true")
            .call()
            .map_err(api_error)?;
        let mut buf = Vec::new();
        resp.into_reader()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            return Err("file too large".into());
        }
        Ok(buf)
    }
}

#[derive(Default, Clone, Serialize)]
pub struct RefreshStatus {
    /// Unix time of the last successful index.
    pub last_ok: Option<u64>,
    /// The error from the last attempt, cleared by a success.
    pub last_error: Option<String>,
}

/// A Drive folder being served: the index of its apps and a download cache.
pub struct Drive {
    client: Arc<DriveClient>,
    pub folder_id: String,
    pub folder_name: String,
    index: Mutex<Index>,
    /// Downloaded bytes keyed by "<fileId>:<md5>", so a changed file is
    /// re-downloaded and an unchanged one never is.
    cache: Mutex<HashMap<String, Vec<u8>>>,
    status: Mutex<RefreshStatus>,
}

impl Drive {
    /// Indexes the folder, then keeps refreshing it in the background. With
    /// `strict`, a failed first index is an error; otherwise the drive starts
    /// empty and the background refresh keeps retrying.
    pub fn connect(
        client: Arc<DriveClient>,
        folder_id: &str,
        folder_name: &str,
        every: Duration,
        strict: bool,
    ) -> Result<Arc<Drive>, String> {
        if !valid_drive_id(folder_id) {
            return Err("folder id looks invalid".into());
        }
        let drive = Arc::new(Drive {
            client,
            folder_id: folder_id.to_string(),
            folder_name: folder_name.to_string(),
            index: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            status: Mutex::new(RefreshStatus::default()),
        });
        match drive.refresh() {
            Ok(()) => println!("drive: indexed {} apps", drive.index.lock().unwrap().len()),
            Err(e) if strict => return Err(e),
            Err(e) => eprintln!("drive: initial refresh failed: {e}"),
        }
        // The loop holds only a weak reference, so it ends by itself once
        // this drive is replaced by another source and dropped.
        let weak = Arc::downgrade(&drive);
        thread::spawn(move || loop {
            thread::sleep(every);
            let Some(me) = weak.upgrade() else { break };
            if let Err(e) = me.refresh() {
                eprintln!("drive: refresh failed (keeping old index): {e}");
            }
        });
        Ok(drive)
    }

    pub fn client(&self) -> &Arc<DriveClient> {
        &self.client
    }

    pub fn status(&self) -> RefreshStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn refresh(&self) -> Result<(), String> {
        let result = self.client.build_index(&self.folder_id);
        let mut status = self.status.lock().unwrap();
        let new = match result {
            Ok(new) => new,
            Err(e) => {
                status.last_error = Some(e.clone());
                return Err(e);
            }
        };
        let live: HashSet<String> = new
            .values()
            .flat_map(|m| m.values())
            .map(|f| format!("{}:{}", f.id, f.md5))
            .collect();
        self.cache.lock().unwrap().retain(|k, _| live.contains(k));
        *self.index.lock().unwrap() = new;
        *status = RefreshStatus { last_ok: Some(unix_now()), last_error: None };
        Ok(())
    }

    fn list_apps(&self) -> Vec<String> {
        let mut v: Vec<String> = self.index.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    /// Only files present in the index can be served, so the web client can
    /// never ask for an arbitrary Drive file ID.
    fn has(&self, app: &str, rel: &str) -> bool {
        self.index.lock().unwrap().get(app).is_some_and(|m| m.contains_key(rel))
    }

    fn read(&self, app: &str, file: &str) -> Option<Vec<u8>> {
        let meta = self.index.lock().unwrap().get(app)?.get(file)?.clone();
        let key = format!("{}:{}", meta.id, meta.md5);
        if let Some(b) = self.cache.lock().unwrap().get(&key) {
            return Some(b.clone());
        }
        match self.client.download(&meta.id) {
            Ok(bytes) => {
                self.cache.lock().unwrap().insert(key, bytes.clone());
                Some(bytes)
            }
            Err(e) => {
                eprintln!("drive: download {app}/{file} failed: {e}");
                None
            }
        }
    }
}
