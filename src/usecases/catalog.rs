//! The live source of apps and the settings behind it. The source can be swapped while the
//! server runs (the local apps folder, or a Google Drive folder), and the choice is saved to
//! `<data dir>/config.json`. Removed apps go to a trash inside the apps folder; imports and the
//! user's permission answers live here too.

use super::history::History;
use super::import::import_zip;
use super::keys::{KeyChecks, KeyEntry, KeyOwner};
use super::workspace::FIRST_RUN_MARKER;
use crate::domain::grants::{self, Answer};
use crate::domain::import_plan::{app_name_from, MAX_ZIP_BYTES};
use crate::domain::package::{app_info, drive_file_id, safe_rel, safe_segment, servable_rel, trash_entry, valid_drive_id, AppInfo, SourceChoice, APP_MARKERS};
use crate::domain::suite;
use crate::ports::{
    assets::Assets,
    clock::Clock,
    drive::{DriveClient, DriveConnector, DriveFolder},
    secrets::Secrets,
    service::Catalog,
    storage::FileSystem,
    web::Downloader,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

/// What is being served.
enum Serving {
    Local,
    Drive(Arc<dyn DriveFolder>),
}

pub struct Hub {
    fs: Arc<dyn FileSystem>,
    drive: Arc<dyn DriveConnector>,
    web: Arc<dyn Downloader>,
    assets: Arc<dyn Assets>,
    data_dir: PathBuf,
    local_root: PathBuf,
    refresh_every: Duration,
    client: Mutex<Option<Arc<dyn DriveClient>>>,
    /// The Drive key: kept through `Secrets` (ADR-2610081500), where it came from, and the file
    /// GDRIVE_SA_KEY names, used when Settings has none.
    secrets: Arc<dyn Secrets>,
    checks: Arc<KeyChecks>,
    key_from: Mutex<Option<&'static str>>,
    key_unreadable: Mutex<Option<String>>,
    env_key: Mutex<Option<PathBuf>>,
    serving: RwLock<Arc<Serving>>,
    /// Serializes changes to grants.json.
    grants_lock: Mutex<()>,
    /// Every save of a local app is a version here (ADR-2610071122).
    history: Arc<History>,
    clock: Arc<dyn Clock>,
}

/// The parts of the outside world the catalog uses.
pub struct HubPorts {
    pub fs: Arc<dyn FileSystem>,
    pub drive: Arc<dyn DriveConnector>,
    pub web: Arc<dyn Downloader>,
    pub assets: Arc<dyn Assets>,
    pub secrets: Arc<dyn Secrets>,
    pub clock: Arc<dyn Clock>,
}

impl Hub {
    pub fn new(ports: HubPorts, checks: Arc<KeyChecks>, history: Arc<History>, data_dir: PathBuf, local_root: PathBuf, refresh_every: Duration) -> Hub {
        Hub {
            fs: ports.fs,
            drive: ports.drive,
            web: ports.web,
            assets: ports.assets,
            data_dir,
            local_root,
            refresh_every,
            client: Mutex::new(None),
            secrets: ports.secrets,
            checks,
            key_from: Mutex::new(None),
            key_unreadable: Mutex::new(None),
            env_key: Mutex::new(None),
            serving: RwLock::new(Arc::new(Serving::Local)),
            grants_lock: Mutex::new(()),
            history,
            clock: ports.clock,
        }
    }

    /// The history of the local apps, for the use cases that save them.
    pub fn history(&self) -> &History {
        &self.history
    }

    fn key_path(&self) -> PathBuf {
        self.data_dir.join("service-account.json")
    }

    fn config_path(&self) -> PathBuf {
        self.data_dir.join("config.json")
    }

    /// Restores the saved state. A key uploaded through the UI wins over the file at `env_key`;
    /// a folder in `env_folder` wins over the saved folder. Returns the line that says where apps
    /// come from, for the composition root to show or log (ADR-2610080930).
    pub fn start(&self, env_key: Option<String>, env_folder: Option<String>) -> String {
        *self.env_key.lock().unwrap() = env_key.map(PathBuf::from);
        self.load_drive_key();

        let cfg: SourceChoice = self.fs.read(&self.config_path()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let (folder, name) = match env_folder {
            Some(id) => (Some(id), None),
            None if cfg.source == "drive" => (cfg.folder_id, cfg.folder_name),
            None => (None, None),
        };
        if let Some(id) = folder {
            let name = name.unwrap_or_else(|| id.clone());
            match self.connect_drive(&id, &name, false) {
                Ok(()) => return format!("source: google drive folder {name} ({id})"),
                Err(e) => eprintln!("drive: not connected, serving local apps: {e}"),
            }
        }
        format!("source: local dir {}", self.local_root.display())
    }

    fn serving(&self) -> Arc<Serving> {
        Arc::clone(&self.serving.read().unwrap())
    }

    fn client(&self) -> Result<Arc<dyn DriveClient>, String> {
        self.client.lock().unwrap().clone().ok_or_else(|| "no service account key uploaded yet".into())
    }

    // ---------- reading the served apps ----------

    fn is_local_app(&self, dir: &Path) -> bool {
        APP_MARKERS.iter().any(|m| self.fs.is_file(&dir.join(m)))
    }

    pub fn list_apps(&self) -> Vec<String> {
        match &*self.serving() {
            Serving::Local => {
                let mut names: Vec<String> = self
                    .fs
                    .list_dir(&self.local_root)
                    .into_iter()
                    .filter(|n| safe_segment(n) && self.is_local_app(&self.local_root.join(n)))
                    .collect();
                names.sort();
                names
            }
            Serving::Drive(d) => d.list_apps(),
        }
    }

    /// A file of a served app. `rel` must already have passed `safe_rel`; a file inside a build
    /// folder (`node_modules`, `target`) is never served, from either source (SPEC.md 3.4).
    pub fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>> {
        if !servable_rel(rel) {
            return None;
        }
        match &*self.serving() {
            Serving::Local => self.fs.read(&self.local_root.join(app).join(rel)),
            Serving::Drive(d) => d.read(app, rel),
        }
    }

    fn has(&self, serving: &Serving, app: &str, rel: &str) -> bool {
        if !servable_rel(rel) {
            return false;
        }
        match serving {
            Serving::Local => self.fs.is_file(&self.local_root.join(app).join(rel)),
            Serving::Drive(d) => d.has(app, rel),
        }
    }

    pub fn apps(&self) -> Vec<AppInfo> {
        let serving = self.serving();
        self.list_apps()
            .into_iter()
            .map(|name| {
                let app = name.clone();
                app_info(name, &|rel| self.read(&app, rel), &|rel| self.has(&serving, &app, rel))
            })
            .collect()
    }

    /// The document for one frame of a suite: an app, or the suite's header when `app` is None.
    pub fn frame(&self, suite_name: &str, app: Option<&str>) -> Result<String, String> {
        suite::frame(&|rel| if safe_rel(rel) { self.read(suite_name, rel) } else { None }, self.assets.frame_shim(), app)
    }

    pub fn status(&self) -> Value {
        let client_email = self.client.lock().unwrap().as_ref().map(|c| c.client_email());
        let apps = self.list_apps().len();
        let mut status = match &*self.serving() {
            Serving::Local => json!({
                "source": "local",
                "version": env!("CARGO_PKG_VERSION"),
                "build": { "commit": env!("WARDIAN_COMMIT"), "date": env!("WARDIAN_COMMIT_DATE") },
                "local_root": self.local_root.display().to_string(),
                "apps": apps,
                "client_email": client_email,
            }),
            Serving::Drive(d) => json!({
                "source": "drive",
                "version": env!("CARGO_PKG_VERSION"),
                "build": { "commit": env!("WARDIAN_COMMIT"), "date": env!("WARDIAN_COMMIT_DATE") },
                "local_root": self.local_root.display().to_string(),
                "folder_id": d.folder_id(),
                "folder_name": d.folder_name(),
                "apps": apps,
                "client_email": client_email,
                "drive_email": d.client_email(),
                "refresh_secs": self.refresh_every.as_secs(),
                "status": d.status(),
                "now": self.clock.now(),
            }),
        };
        status["first_run"] = json!(self.first_run());
        status
    }

    /// True until the first-run setup is skipped or finished (ADR-2610072033). The marker is left
    /// by a start with an empty data folder (`workspace::mark_first_run`).
    pub fn first_run(&self) -> bool {
        self.fs.is_file(&self.data_dir.join(FIRST_RUN_MARKER))
    }

    /// The first-run setup was skipped or finished: it is not shown again, in any browser.
    pub fn setup_done(&self) -> Result<Value, String> {
        self.fs.remove_file(&self.data_dir.join(FIRST_RUN_MARKER));
        Ok(json!({ "first_run": false }))
    }

    // ---------- Google Drive ----------

    /// Loads the Drive key: the one saved in Settings, else the file GDRIVE_SA_KEY names.
    fn load_drive_key(&self) {
        let saved = match self.secrets.read(&self.key_path()) {
            Ok(saved) => saved.map(|b| (b, "settings")),
            Err(e) => {
                eprintln!("drive: the saved key cannot be read: {e}");
                *self.key_unreadable.lock().unwrap() = Some(e);
                None
            }
        };
        let env = || self.env_key.lock().unwrap().clone().and_then(|p| self.fs.read(&p).map(|b| (b, "environment")));
        let (client, from) = match saved.or_else(env) {
            Some((raw, from)) => match self.drive.client(&String::from_utf8_lossy(&raw)) {
                Ok(c) => (Some(c), Some(from)),
                Err(e) => {
                    eprintln!("drive: key from {from}: {e}");
                    (None, None)
                }
            },
            None => (None, None),
        };
        *self.client.lock().unwrap() = client;
        *self.key_from.lock().unwrap() = from;
    }

    /// Checks a key against Google before saving it, so a bad key never
    /// replaces a good one. Returns the address to share folders with.
    pub fn set_key(&self, raw: &str) -> Result<String, String> {
        let client = self.drive.client(raw)?;
        let checked = client.check();
        self.checks.record("drive", &checked);
        checked?;
        self.secrets.write(&self.key_path(), raw.as_bytes()).map_err(|e| format!("saving key: {e}"))?;
        let email = client.client_email();
        *self.client.lock().unwrap() = Some(client);
        *self.key_from.lock().unwrap() = Some("settings");
        *self.key_unreadable.lock().unwrap() = None;
        Ok(email)
    }

    pub fn browse(&self, parent: Option<&str>) -> Result<Value, String> {
        Ok(json!({ "folders": self.client()?.browse(parent)? }))
    }

    pub fn preview(&self, folder_id: &str) -> Result<Value, String> {
        Ok(json!({ "apps": self.client()?.preview(folder_id)? }))
    }

    pub fn refresh(&self) -> Result<(), String> {
        match &*self.serving() {
            Serving::Local => Ok(()),
            Serving::Drive(d) => d.refresh(),
        }
    }

    pub fn use_local(&self) -> Result<(), String> {
        *self.serving.write().unwrap() = Arc::new(Serving::Local);
        self.save(SourceChoice { source: "local".into(), ..Default::default() })
    }

    pub fn use_drive(&self, folder_id: &str, folder_name: &str) -> Result<(), String> {
        self.connect_drive(folder_id, folder_name, true)?;
        self.save(SourceChoice { source: "drive".into(), folder_id: Some(folder_id.into()), folder_name: Some(folder_name.into()) })
    }

    fn connect_drive(&self, folder_id: &str, folder_name: &str, strict: bool) -> Result<(), String> {
        if !valid_drive_id(folder_id) {
            return Err("folder id looks invalid".into());
        }
        let folder = self.client()?.open_folder(folder_id, folder_name, self.refresh_every, strict)?;
        // The old source is dropped here; if it was a Drive folder, its background refresh
        // notices and stops.
        *self.serving.write().unwrap() = Arc::new(Serving::Drive(folder));
        Ok(())
    }

    fn save(&self, cfg: SourceChoice) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
        self.fs.write_private(&self.config_path(), &body).map_err(|e| format!("saving config: {e}"))
    }

    // ---------- the local apps folder ----------

    /// The apps folder on this machine, where imports and AI-made apps go.
    pub fn local_root(&self) -> &Path {
        &self.local_root
    }

    pub fn serving_local(&self) -> bool {
        matches!(&*self.serving(), Serving::Local)
    }

    /// True when the local apps folder has an app called `name`.
    pub fn local_app_exists(&self, name: &str) -> bool {
        safe_segment(name) && self.is_local_app(&self.local_root.join(name))
    }

    /// Removed apps wait here, inside the apps folder (so a move is one rename). The folder is
    /// hidden, so the host never lists or serves it.
    fn trash_dir(&self) -> PathBuf {
        self.local_root.join(".trash")
    }

    /// Moves a local app to the trash. Nothing is deleted: `restore_app` puts it back, and only
    /// the user empties the trash, outside Wardian.
    pub fn remove_app(&self, name: &str) -> Result<Value, String> {
        if !self.serving_local() {
            return Err("these apps come from Google Drive, which Wardian only reads. Remove the app's folder in Drive.".into());
        }
        let id = self.move_to_trash(name)?;
        Ok(json!({ "removed": name, "id": id }))
    }

    /// Moves a local app folder into the trash and returns its trash id.
    fn move_to_trash(&self, name: &str) -> Result<String, String> {
        if !safe_segment(name) {
            return Err(format!("\"{name}\" is not an app name"));
        }
        let dir = self.local_root.join(name);
        if !self.fs.exists(&dir) {
            return Err(format!("no app \"{name}\""));
        }
        if !self.fs.is_real_dir(&dir) || !self.is_local_app(&dir) {
            return Err(format!("\"{name}\" is not an app folder"));
        }
        self.fs.create_dir_all(&self.trash_dir()).map_err(|e| format!("creating the trash: {e}"))?;
        let mut id = format!("{name}--{}", self.clock.now());
        while self.fs.exists(&self.trash_dir().join(&id)) {
            id.push('_');
        }
        self.fs.rename(&dir, &self.trash_dir().join(&id)).map_err(|e| format!("removing {name}: {e}"))?;
        println!("removed: {name} (kept in {})", self.trash_dir().join(&id).display());
        Ok(id)
    }

    /// Puts a removed app back, unless an app with its name exists again.
    pub fn restore_app(&self, id: &str) -> Result<Value, String> {
        let name = id.rsplit_once("--").map(|(n, _)| n).unwrap_or("");
        if !safe_segment(id) || !safe_segment(name) {
            return Err("not a removed app".into());
        }
        let from = self.trash_dir().join(id);
        if !self.fs.is_dir(&from) {
            return Err("that app is no longer in the trash".into());
        }
        let to = self.local_root.join(name);
        if self.fs.exists(&to) {
            return Err(format!("an app named \"{name}\" exists again; rename or remove it first"));
        }
        self.fs.rename(&from, &to).map_err(|e| format!("restoring {name}: {e}"))?;
        println!("restored: {name}");
        self.history.record(name, "restore", "put back from the removed apps");
        Ok(json!({ "restored": name }))
    }

    /// The removed apps, newest first.
    pub fn trash(&self) -> Value {
        let mut items: Vec<(u64, String, String)> =
            self.fs.list_dir(&self.trash_dir()).into_iter().filter_map(|id| trash_entry(&id).map(|(t, name)| (t, name, id))).collect();
        items.sort_by(|a, b| b.0.cmp(&a.0));
        json!({
            "folder": self.trash_dir().display().to_string(),
            "items": items.into_iter().map(|(t, name, id)| json!({ "id": id, "name": name, "removed_at": t })).collect::<Vec<_>>(),
        })
    }

    // ---------- import ----------

    /// Unpacks a zip into the local apps folder. The apps show up once the
    /// local folder is the source; the reply says whether it is.
    pub fn import(&self, bytes: &[u8], zip_name: &str, replace: bool) -> Result<Value, String> {
        let history = &self.history;
        let done = import_zip(&*self.fs, &*self.clock, bytes, zip_name, &self.local_root, replace, &|app| {
            history.before_change(app);
        })?;
        println!("import: {} -> {}", zip_name, done.apps.join(", "));
        for app in &done.apps {
            history.record(app, "import", &format!("imported from {zip_name}"));
        }
        Ok(json!({ "apps": done.apps, "skipped": done.skipped, "serving_local": self.serving_local() }))
    }

    /// Fetches a zip from a link, then imports it. A Google Drive file link is
    /// fetched with the service account, so private files work if shared.
    pub fn import_url(&self, url: &str, replace: bool) -> Result<Value, String> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("the link must start with https:// or http://".into());
        }
        let (bytes, name) = match drive_file_id(url) {
            Some(id) => (self.client()?.download(&id).map_err(|e| format!("Drive download failed: {e}"))?, id),
            None => {
                let buf = self.web.fetch(url, MAX_ZIP_BYTES)?;
                if buf.len() as u64 > MAX_ZIP_BYTES {
                    return Err("the zip is larger than 100 MB".into());
                }
                let path = url.split(['?', '#']).next().unwrap_or("");
                (buf, path.rsplit('/').next().unwrap_or("").to_string())
            }
        };
        let name = if app_name_from(&name).is_empty() { "imported".to_string() } else { name };
        self.import(&bytes, &name, replace)
    }

    // ---------- permissions ----------

    fn grants_path(&self) -> PathBuf {
        self.data_dir.join("grants.json")
    }

    fn grant_list(&self) -> Vec<Value> {
        self.fs.read(&self.grants_path()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_array().cloned()).unwrap_or_default()
    }

    /// The user's answers to permission questions.
    pub fn grants(&self) -> Value {
        json!({ "grants": self.grant_list() })
    }

    /// Saves one answer: decision "allow" or "deny", or "ask" to forget it.
    pub fn set_grant(&self, body: &Value) -> Result<Value, String> {
        let answer = Answer::from_json(body);
        let _guard = self.grants_lock.lock().unwrap();
        let list = grants::apply(self.grant_list(), &answer, self.clock.now())?;
        let bytes = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
        self.fs.write_private(&self.grants_path(), &bytes).map_err(|e| format!("saving permissions: {e}"))?;
        println!("permission: {} {} {}: {}", answer.app, answer.mode, answer.channel, answer.decision);
        Ok(json!({ "grants": list }))
    }

    /// A host capability call is allowed only when `app` in suite `package`
    /// declares `cap` in suite.json and the user allowed the package to use
    /// `grant` (the name the user is asked about: "splunk", or "ai" for claude:sample).
    pub fn check_host_cap(&self, package: &str, app: &str, cap: &str, grant: &str) -> Result<(), String> {
        if !safe_segment(package) {
            return Err("not an app name".into());
        }
        let suite: Value = self.read(package, "suite.json").and_then(|b| serde_json::from_slice(&b).ok()).ok_or_else(|| format!("{package} is not a suite"))?;
        if !grants::declares_cap(&suite, app, cap) {
            return Err(format!("{package}/{app} does not declare the capability \"{cap}\" in suite.json"));
        }
        // An empty grant: the capability needs only the declaration (an app's own database).
        if !grant.is_empty() && !grants::granted(&self.grant_list(), package, grant, "use") {
            return Err(format!("you have not allowed {package} to use {grant}"));
        }
        Ok(())
    }
}

impl KeyOwner for Hub {
    fn entries(&self) -> Vec<KeyEntry> {
        let from = *self.key_from.lock().unwrap();
        let detail = match &*self.client.lock().unwrap() {
            Some(c) => format!("service account {}", c.client_email()),
            None => "not set".into(),
        };
        let error = self.key_unreadable.lock().unwrap().clone();
        vec![KeyEntry { id: "drive", name: "Google Drive service account", from, detail, error }]
    }

    fn retest(&self, id: &str) -> Option<Result<(), String>> {
        (id == "drive").then(|| {
            let out = self.client().and_then(|c| c.check());
            self.checks.record("drive", &out);
            out
        })
    }

    /// Removes the saved key. While Drive is the app source, Wardian serves the local apps after.
    fn forget(&self, id: &str) -> Option<Result<(), String>> {
        (id == "drive").then(|| {
            self.secrets.remove(&self.key_path());
            *self.key_unreadable.lock().unwrap() = None;
            if matches!(&*self.serving(), Serving::Drive(_)) {
                self.use_local()?;
            }
            self.load_drive_key();
            Ok(())
        })
    }
}

impl Catalog for Hub {
    fn list_apps(&self) -> Vec<String> {
        Hub::list_apps(self)
    }
    fn apps(&self) -> Vec<AppInfo> {
        Hub::apps(self)
    }
    fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>> {
        Hub::read(self, app, rel)
    }
    fn frame(&self, suite: &str, app: Option<&str>) -> Result<String, String> {
        Hub::frame(self, suite, app)
    }
    fn status(&self) -> Value {
        Hub::status(self)
    }
    fn grants(&self) -> Value {
        Hub::grants(self)
    }
    fn set_grant(&self, body: &Value) -> Result<Value, String> {
        Hub::set_grant(self, body)
    }
    fn setup_done(&self) -> Result<Value, String> {
        Hub::setup_done(self)
    }
    fn check_host_cap(&self, package: &str, app: &str, cap: &str, grant: &str) -> Result<(), String> {
        Hub::check_host_cap(self, package, app, cap, grant)
    }
    fn trash(&self) -> Value {
        Hub::trash(self)
    }
    fn remove_app(&self, name: &str) -> Result<Value, String> {
        Hub::remove_app(self, name)
    }
    fn restore_app(&self, id: &str) -> Result<Value, String> {
        Hub::restore_app(self, id)
    }
    fn import(&self, bytes: &[u8], zip_name: &str, replace: bool) -> Result<Value, String> {
        Hub::import(self, bytes, zip_name, replace)
    }
    fn import_url(&self, url: &str, replace: bool) -> Result<Value, String> {
        Hub::import_url(self, url, replace)
    }
    fn refresh(&self) -> Result<(), String> {
        Hub::refresh(self)
    }
    fn set_drive_key(&self, raw: &str) -> Result<String, String> {
        self.set_key(raw)
    }
    fn browse(&self, parent: Option<&str>) -> Result<Value, String> {
        Hub::browse(self, parent)
    }
    fn preview(&self, folder_id: &str) -> Result<Value, String> {
        Hub::preview(self, folder_id)
    }
    fn use_local(&self) -> Result<(), String> {
        Hub::use_local(self)
    }
    fn use_drive(&self, folder_id: &str, folder_name: &str) -> Result<(), String> {
        Hub::use_drive(self, folder_id, folder_name)
    }
}
