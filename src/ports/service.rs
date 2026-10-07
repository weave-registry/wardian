//! The driving ports: what the web server may ask Wardian to do. The use cases implement them;
//! the HTTP adapter only knows these traits and the types they carry.

pub use crate::domain::import_plan::MAX_ZIP_BYTES;
pub use crate::domain::package::{safe_rel, safe_segment, AppInfo};
pub use crate::domain::suite::FRAME_CSP;
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
    fn search(&self, spl: &str, earliest: &str, latest: &str) -> Result<Value, String>;
    /// Runs a search and loads its results into `table` of `package`'s database, in chunks:
    /// {table, columns, fields, total, truncated, seconds, messages}.
    fn search_into(&self, package: &str, table: &str, spl: &str, earliest: &str, latest: &str) -> Result<Value, String>;
}

/// The docs, the JSON Schemas and the component library, as pages.
pub trait Pages: Send + Sync {
    fn page(&self, name: &str) -> Option<String>;
    fn schema(&self, name: &str) -> Option<&'static str>;
    fn ui_file(&self, name: &str) -> Option<&'static str>;
    fn gallery(&self) -> &'static str;
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
    fn channel(&self, channel: &str) -> Value;
    /// Keeps the latest message on a channel; null forgets it.
    fn set_channel(&self, channel: &str, message: Value) -> Result<Value, String>;
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
    pub builder: Arc<dyn Builder>,
    pub searches: Arc<dyn Searches>,
    pub pages: Arc<dyn Pages>,
}
