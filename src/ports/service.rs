//! The driving ports: what the web server may ask Wardian to do. The use cases implement them;
//! the HTTP adapter only knows these traits and the types they carry.

pub use crate::domain::import_plan::MAX_ZIP_BYTES;
pub use crate::domain::package::{safe_rel, safe_segment, AppInfo};
pub use crate::domain::suite::{page_csp, FRAME_CSP};
pub use crate::domain::export::{file_name as export_file_name, MIME as EXPORT_MIME};
pub use crate::domain::jobs::JobKind;
use serde_json::Value;
use std::sync::Arc;

/// The apps being served, their settings, the trash, imports and permissions.
pub trait Catalog: Send + Sync {
    fn list_apps(&self) -> Vec<String>;
    fn apps(&self) -> Vec<AppInfo>;
    /// A file of a served app; `rel` must already have passed `safe_rel`.
    fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>>;
    /// The document for one frame of a suite, or its header when `app` is None.
    fn frame(&self, suite: &str, app: Option<&str>) -> Result<String, String>;
    fn status(&self) -> Value;
    fn grants(&self) -> Value;
    fn set_grant(&self, body: &Value) -> Result<Value, String>;
    /// The first-run setup was skipped or finished (ADR-2610072033).
    fn setup_done(&self) -> Result<Value, String>;
    /// Whether `app` of suite `package` declares `cap` and the user allowed `grant` for it.
    fn check_host_cap(&self, package: &str, app: &str, cap: &str, grant: &str) -> Result<(), String>;
    fn trash(&self) -> Value;
    fn remove_app(&self, name: &str) -> Result<Value, String>;
    fn restore_app(&self, id: &str) -> Result<Value, String>;
    fn import(&self, bytes: &[u8], zip_name: &str, replace: bool) -> Result<Value, String>;
    fn import_url(&self, url: &str, replace: bool) -> Result<Value, String>;
    fn refresh(&self) -> Result<(), String>;
    /// Tests and saves a Google service account key; returns the address to share folders with.
    fn set_drive_key(&self, raw: &str) -> Result<String, String>;
    fn browse(&self, parent: Option<&str>) -> Result<Value, String>;
    fn preview(&self, folder_id: &str) -> Result<Value, String>;
    fn use_local(&self) -> Result<(), String>;
    fn use_drive(&self, folder_id: &str, folder_name: &str) -> Result<(), String>;
}

/// "Make an app", and Claude for apps (`claude:sample`).
pub trait Builder: Send + Sync {
    fn status(&self) -> Value;
    fn set_key(&self, key: &str, workspace: Option<&str>) -> Result<Value, String>;
    /// Chooses where Claude is reached: the Anthropic API or Amazon Bedrock (ADR-2610071106).
    fn set_provider(&self, body: &Value) -> Result<Value, String>;
    fn send(&self, body: &Value) -> Result<Value, String>;
    fn sample(&self, body: &Value) -> Result<Value, String>;
    fn stop(&self, session: &str) -> Result<Value, String>;
    fn claim_test(&self, session: &str, saved: usize) -> Result<Value, String>;
    fn tested(&self, session: &str, body: &Value) -> Result<Value, String>;
    fn sessions(&self) -> Value;
    fn events(&self, session: &str, since: usize) -> Result<Value, String>;
}

/// Splunk searches for apps, and the Splunk account in Settings.
pub trait Searches: Send + Sync {
    fn status(&self) -> Value;
    fn set_config(&self, body: &Value) -> Result<Value, String>;
    /// `watch` says when to stop: the search job on Splunk is then cancelled too.
    fn search(&self, spl: &str, earliest: &str, latest: &str, watch: &dyn Watch) -> Result<Value, String>;
    /// Runs a search and loads its results into `table` of `package`'s database, in chunks:
    /// {table, columns, fields, total, truncated, seconds, messages}. Reports rows loaded to `watch`.
    fn search_into(&self, package: &str, table: &str, spl: &str, earliest: &str, latest: &str, watch: &dyn Watch) -> Result<Value, String>;
}

/// What a long call sees of the job it runs in (ADR-2610072118): whether to stop, and where to
/// say how far it has got.
pub trait Watch: Send + Sync {
    fn stopped(&self) -> bool;
    /// Rows loaded so far.
    fn progress(&self, rows: u64);
}

/// A call made in the foreground, by a request that waits for it: never stopped, nobody watching.
pub struct Unwatched;

impl Watch for Unwatched {
    fn stopped(&self) -> bool {
        false
    }
    fn progress(&self, _: u64) {}
}

/// Long calls as background jobs (ADR-2610072118). The web server checks permissions before
/// `start`; a job is shown only to the package that started it, or to the admin (`package` None).
pub trait Jobs: Send + Sync {
    /// Starts `kind` with the request `body` on its own thread and answers {job: id} at once.
    fn start(&self, kind: JobKind, package: &str, app: &str, body: &Value) -> Result<Value, String>;
    /// {jobs: [...]}, newest first, without results.
    fn list(&self, package: Option<&str>) -> Value;
    /// One job, with its result once it is done.
    fn get(&self, id: &str, package: Option<&str>) -> Result<Value, String>;
    /// Stops a running job; a Splunk search is cancelled on the Splunk server too.
    fn cancel(&self, id: &str, package: Option<&str>) -> Result<Value, String>;
}

/// The docs, the JSON Schemas and the component library, as pages.
pub trait Pages: Send + Sync {
    fn page(&self, name: &str) -> Option<String>;
    /// Every file of the static docs site, as (path, contents), for `wardian docs`.
    fn site(&self) -> Vec<(String, String)>;
    fn schema(&self, name: &str) -> Option<&'static str>;
    fn ui_file(&self, name: &str) -> Option<&'static str>;
    fn gallery(&self) -> &'static str;
    /// The AI skills' names, and every file `wardian skills` installs, as (path under
    /// `.claude/skills/`, text) (ADR-2610080928).
    fn skill_names(&self) -> Vec<&'static str>;
    fn skills(&self) -> Vec<(String, String)>;
}

/// The viewer's state, kept by the host (ADR-2610071055): Arrange layouts, each suite app's saved
/// data, and the latest message per channel.
pub trait ViewerState: Send + Sync {
    fn layout(&self, package: &str) -> Result<Value, String>;
    /// Keeps a layout; null forgets it.
    fn set_layout(&self, package: &str, layout: Value) -> Result<Value, String>;
    /// {app: {key: value}} for one package.
    fn app_data(&self, package: &str) -> Result<Value, String>;
    /// Sets one key of one app; null removes it.
    fn set_app_value(&self, package: &str, app: &str, key: &str, value: Value) -> Result<Value, String>;
    /// Adds what a browser held that the host does not have yet; returns the package's data.
    fn merge_app_data(&self, package: &str, data: &Value) -> Result<Value, String>;
    /// Replaces all of a package's data: what an imported `.wardian` file brought with it.
    fn replace_app_data(&self, package: &str, data: &Value) -> Result<Value, String>;
    fn channel(&self, channel: &str) -> Value;
    /// Keeps the latest message on a channel; null forgets it.
    fn set_channel(&self, channel: &str, message: Value) -> Result<Value, String>;
}

/// An app as a `.wardian` file (ADR-2610071248): export, and what an import brings.
pub trait Exports: Send + Sync {
    /// What an export of `app` would hold, without building it: files, sizes, data, what never goes in.
    fn preview(&self, app: &str, with_data: bool) -> Result<Value, String>;
    /// The `.wardian` file; refused when the app does not pass `wardian check`.
    fn export(&self, app: &str, with_data: bool) -> Result<Vec<u8>, String>;
    /// What a file holds, before anything is installed: its manifest, what the app may use, its data.
    fn preview_import(&self, bytes: &[u8]) -> Result<Value, String>;
    /// Before an import with data replaces `app`, keeps its current data next to its latest version.
    fn keep_data_before_import(&self, app: &str) -> Result<(), String>;
    /// Installs the data a file carries for `app` (the installed name), replacing what it had.
    fn install_data(&self, app: &str, bytes: &[u8]) -> Result<Value, String>;
}

/// The history of each local app (ADR-2610071122): its versions, what changed, and restoring one.
pub trait AppHistory: Send + Sync {
    /// {app, versions: [{n, at, by, why, current}]}, newest first.
    fn versions(&self, app: &str) -> Result<Value, String>;
    /// {app, n, files: [{path, status, diff}]}: version `n` against the app as it is now.
    fn diff(&self, app: &str, n: u64) -> Result<Value, String>;
    /// Puts version `n` back, recorded as a new version.
    fn restore(&self, app: &str, n: u64) -> Result<Value, String>;
}

/// Each package's own SQLite database, for apps that declare `db` (ADR-2610071219). The web server
/// checks the declaration and the user's permission first.
pub trait Tables: Send + Sync {
    /// One statement in the package's database. `params` is a list.
    fn query(&self, package: &str, sql: &str, params: &Value) -> Result<Value, String>;
    /// One page of a table. `source` names another package to read from, read-only.
    fn page(&self, package: &str, source: Option<&str>, request: &Value) -> Result<Value, String>;
    /// Rows into a table: {table, columns, rows, create, replace}.
    fn insert(&self, package: &str, body: &Value) -> Result<Value, String>;
    fn tables(&self, package: &str) -> Result<Value, String>;
}

/// Everything the web server serves.
#[derive(Clone)]
pub struct Services {
    pub catalog: Arc<dyn Catalog>,
    pub state: Arc<dyn ViewerState>,
    pub tables: Arc<dyn Tables>,
    pub history: Arc<dyn AppHistory>,
    pub exports: Arc<dyn Exports>,
    pub builder: Arc<dyn Builder>,
    pub searches: Arc<dyn Searches>,
    pub jobs: Arc<dyn Jobs>,
    pub pages: Arc<dyn Pages>,
}
