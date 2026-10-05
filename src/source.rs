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
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

/// Accept only simple names: no slashes, no "..", no hidden files.
pub fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
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
                            .filter(|e| e.path().join("app.wasm").is_file())
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

    pub fn read(&self, app: &str, file: &str) -> Option<Vec<u8>> {
        match self {
            Source::Local(root) => fs::read(root.join(app).join(file)).ok(),
            Source::Drive(d) => d.read(app, file),
        }
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

#[derive(Clone)]
struct FileMeta {
    id: String,
    md5: String,
}

/// app name -> (file name -> Drive file)
type Index = HashMap<String, HashMap<String, FileMeta>>;

pub struct Drive {
    sa: ServiceAccount,
    folder_id: String,
    api_base: String,
    agent: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
    index: Mutex<Index>,
    /// Downloaded bytes keyed by "<fileId>:<md5>", so a changed file is
    /// re-downloaded and an unchanged one never is.
    cache: Mutex<HashMap<String, Vec<u8>>>,
}

impl Drive {
    pub fn new(key_path: &str, folder_id: &str, api_base: &str) -> Result<Arc<Drive>, String> {
        let raw = fs::read_to_string(key_path).map_err(|e| format!("reading {key_path}: {e}"))?;
        let sa: ServiceAccount =
            serde_json_from(&raw).map_err(|e| format!("parsing service account key: {e}"))?;
        if folder_id.is_empty()
            || !folder_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("GDRIVE_FOLDER_ID looks invalid".into());
        }
        Ok(Arc::new(Drive {
            sa,
            folder_id: folder_id.to_string(),
            api_base: api_base.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build(),
            token: Mutex::new(None),
            index: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }))
    }

    /// Initial index build, then refresh in the background.
    pub fn start(self: &Arc<Self>, every: Duration) {
        match self.refresh() {
            Ok(()) => println!("drive: indexed {} apps", self.index.lock().unwrap().len()),
            Err(e) => eprintln!("drive: initial refresh failed: {e}"),
        }
        let me = Arc::clone(self);
        thread::spawn(move || loop {
            thread::sleep(every);
            if let Err(e) = me.refresh() {
                eprintln!("drive: refresh failed (keeping old index): {e}");
            }
        });
    }

    fn token(&self) -> Result<String, String> {
        let mut guard = self.token.lock().unwrap();
        if let Some((t, exp)) = &*guard {
            if Instant::now() < *exp {
                return Ok(t.clone());
            }
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
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
            .map_err(|e| format!("token request failed: {e}"))?
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
                .query("fields", "nextPageToken,files(id,name,md5Checksum)")
                .query("pageSize", "1000")
                .query("supportsAllDrives", "true")
                .query("includeItemsFromAllDrives", "true");
            if let Some(p) = &page {
                req = req.query("pageToken", p);
            }
            let list: FileList = req
                .call()
                .map_err(|e| format!("drive list failed: {e}"))?
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

    fn refresh(&self) -> Result<(), String> {
        let folders = self.list(&format!(
            "'{}' in parents and mimeType='{FOLDER_MIME}' and trashed=false",
            self.folder_id
        ))?;
        let mut new: Index = HashMap::new();
        for f in folders {
            if !safe_segment(&f.name) || new.contains_key(&f.name) {
                continue;
            }
            let files = self.list(&format!(
                "'{}' in parents and mimeType!='{FOLDER_MIME}' and trashed=false",
                f.id
            ))?;
            let mut m: HashMap<String, FileMeta> = HashMap::new();
            for file in files {
                if let (true, Some(md5)) = (safe_segment(&file.name), file.md5_checksum) {
                    m.entry(file.name).or_insert(FileMeta { id: file.id, md5 });
                }
            }
            if m.contains_key("app.wasm") {
                new.insert(f.name, m);
            }
        }
        let live: HashSet<String> = new
            .values()
            .flat_map(|m| m.values())
            .map(|f| format!("{}:{}", f.id, f.md5))
            .collect();
        self.cache.lock().unwrap().retain(|k, _| live.contains(k));
        *self.index.lock().unwrap() = new;
        Ok(())
    }

    fn list_apps(&self) -> Vec<String> {
        let mut v: Vec<String> = self.index.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    /// Only files present in the index can be served, so the web client can
    /// never ask for an arbitrary Drive file ID.
    fn read(&self, app: &str, file: &str) -> Option<Vec<u8>> {
        let meta = self.index.lock().unwrap().get(app)?.get(file)?.clone();
        let key = format!("{}:{}", meta.id, meta.md5);
        if let Some(b) = self.cache.lock().unwrap().get(&key) {
            return Some(b.clone());
        }
        match self.download(&meta.id) {
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

    fn download(&self, id: &str) -> Result<Vec<u8>, String> {
        let tok = self.token()?;
        let resp = self
            .agent
            .get(&format!("{}/drive/v3/files/{id}", self.api_base))
            .set("Authorization", &format!("Bearer {tok}"))
            .query("alt", "media")
            .query("supportsAllDrives", "true")
            .call()
            .map_err(|e| format!("{e}"))?;
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

// Tiny helper so we don't need serde_json as a direct dependency beyond ureq's.
fn serde_json_from<T: for<'de> Deserialize<'de>>(s: &str) -> Result<T, String> {
    ureq::serde_json::from_str(s).map_err(|e| e.to_string())
}
