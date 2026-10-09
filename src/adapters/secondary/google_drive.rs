//! Google Drive as a source of apps: a service account reads a folder through the Drive API
//! directly. Each subfolder that holds an app is an app; files are downloaded on first use and
//! cached by checksum, and the index is refreshed in the background.

use crate::ports::drive::{
    safe_segment, valid_drive_id, DriveClient, DriveConnector, DriveFolder, Folder, RefreshStatus, APP_MARKERS, MAX_APP_FILES, MAX_DEPTH, SKIP_DIRS,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Seconds since 1970, for the token's claims and the refresh status. An adapter reads the system
/// clock itself; the core asks the `Clock` port (ADR-2610091040).
fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

/// Makes Drive clients that talk to `api_base` (Google's, or a test server's).
pub struct GoogleDrive {
    api_base: String,
}

impl GoogleDrive {
    pub fn new(api_base: &str) -> GoogleDrive {
        GoogleDrive { api_base: api_base.to_string() }
    }
}

impl DriveConnector for GoogleDrive {
    fn client(&self, key_json: &str) -> Result<Arc<dyn DriveClient>, String> {
        Ok(GoogleClient::from_json(key_json, &self.api_base)?)
    }
}

impl DriveClient for GoogleClient {
    fn client_email(&self) -> String {
        self.sa.client_email.clone()
    }
    fn check(&self) -> Result<(), String> {
        self.token().map(|_| ())
    }
    fn browse(&self, parent: Option<&str>) -> Result<Vec<Folder>, String> {
        self.browse_folders(parent)
    }
    fn preview(&self, folder_id: &str) -> Result<Vec<String>, String> {
        self.preview_folder(folder_id)
    }
    fn download(&self, file_id: &str) -> Result<Vec<u8>, String> {
        self.download_file(file_id)
    }
    fn open_folder(self: Arc<Self>, folder_id: &str, folder_name: &str, every: Duration, strict: bool) -> Result<Arc<dyn DriveFolder>, String> {
        Ok(ServedFolder::connect(self, folder_id, folder_name, every, strict)?)
    }
}

impl DriveFolder for ServedFolder {
    fn folder_id(&self) -> String {
        self.folder_id.clone()
    }
    fn folder_name(&self) -> String {
        self.folder_name.clone()
    }
    fn client_email(&self) -> String {
        self.client.sa.client_email.clone()
    }
    fn status(&self) -> RefreshStatus {
        self.status.lock().unwrap().clone()
    }
    fn refresh(&self) -> Result<(), String> {
        self.refresh_index()
    }
    fn list_apps(&self) -> Vec<String> {
        self.app_names()
    }
    fn has(&self, app: &str, rel: &str) -> bool {
        self.indexed(app, rel)
    }
    fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>> {
        self.fetch(app, rel)
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

#[derive(Clone)]
struct FileMeta {
    id: String,
    md5: String,
}

/// app name -> (file name -> Drive file)
type Index = HashMap<String, HashMap<String, FileMeta>>;

/// An authenticated connection to the Drive API. It is not tied to a folder,
/// so the settings page can browse Drive before an apps folder is chosen.
struct GoogleClient {
    sa: ServiceAccount,
    api_base: String,
    agent: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
}

impl GoogleClient {
    fn from_json(raw: &str, api_base: &str) -> Result<Arc<GoogleClient>, String> {
        let sa: ServiceAccount =
            serde_json::from_str(raw).map_err(|e| format!("not a service account key: {e}"))?;
        EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
            .map_err(|e| format!("bad private key: {e}"))?;
        Ok(Arc::new(GoogleClient {
            sa,
            api_base: api_base.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build(),
            token: Mutex::new(None),
        }))
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
    fn browse_folders(&self, parent: Option<&str>) -> Result<Vec<Folder>, String> {
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
    fn preview_folder(&self, folder_id: &str) -> Result<Vec<String>, String> {
        let mut v: Vec<String> = self.build_index(folder_id)?.into_keys().collect();
        v.sort();
        Ok(v)
    }

    fn download_file(&self, id: &str) -> Result<Vec<u8>, String> {
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

/// A Drive folder being served: the index of its apps and a download cache.
struct ServedFolder {
    client: Arc<GoogleClient>,
    folder_id: String,
    folder_name: String,
    index: Mutex<Index>,
    /// Downloaded bytes keyed by "<fileId>:<md5>", so a changed file is
    /// re-downloaded and an unchanged one never is.
    cache: Mutex<HashMap<String, Vec<u8>>>,
    status: Mutex<RefreshStatus>,
}

impl ServedFolder {
    /// Indexes the folder, then keeps refreshing it in the background. With
    /// `strict`, a failed first index is an error; otherwise the drive starts
    /// empty and the background refresh keeps retrying.
    fn connect(client: Arc<GoogleClient>, folder_id: &str, folder_name: &str, every: Duration, strict: bool) -> Result<Arc<ServedFolder>, String> {
        if !valid_drive_id(folder_id) {
            return Err("folder id looks invalid".into());
        }
        let drive = Arc::new(ServedFolder {
            client,
            folder_id: folder_id.to_string(),
            folder_name: folder_name.to_string(),
            index: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            status: Mutex::new(RefreshStatus::default()),
        });
        match drive.refresh_index() {
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
            if let Err(e) = me.refresh_index() {
                eprintln!("drive: refresh failed (keeping old index): {e}");
            }
        });
        Ok(drive)
    }

    fn refresh_index(&self) -> Result<(), String> {
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

    fn app_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.index.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    /// Only files present in the index can be served, so the web client can
    /// never ask for an arbitrary Drive file ID.
    fn indexed(&self, app: &str, rel: &str) -> bool {
        self.index.lock().unwrap().get(app).is_some_and(|m| m.contains_key(rel))
    }

    fn fetch(&self, app: &str, file: &str) -> Option<Vec<u8>> {
        let meta = self.index.lock().unwrap().get(app)?.get(file)?.clone();
        let key = format!("{}:{}", meta.id, meta.md5);
        if let Some(b) = self.cache.lock().unwrap().get(&key) {
            return Some(b.clone());
        }
        match self.client.download_file(&meta.id) {
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
