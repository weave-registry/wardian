//! Google Drive as a source of apps, through a service account. Only the adapter talks to Google.

pub use crate::domain::package::{safe_segment, valid_drive_id, Folder, RefreshStatus, APP_MARKERS, MAX_APP_FILES, MAX_DEPTH, SKIP_DIRS};
use std::{sync::Arc, time::Duration};

/// Makes a client from a service account key file (its JSON text).
pub trait DriveConnector: Send + Sync {
    fn client(&self, key_json: &str) -> Result<Arc<dyn DriveClient>, String>;
}

/// An authenticated connection to the Drive API, not tied to a folder, so the settings page can
/// browse Drive before an apps folder is chosen.
pub trait DriveClient: Send + Sync {
    /// The address a Drive folder must be shared with.
    fn client_email(&self) -> String;
    /// Proves the key works by fetching an access token.
    fn check(&self) -> Result<(), String>;
    /// Subfolders of `parent`; with none, the folders shared with the account and its shared drives.
    fn browse(&self, parent: Option<&str>) -> Result<Vec<Folder>, String>;
    /// The app names a folder would serve, without serving it.
    fn preview(&self, folder_id: &str) -> Result<Vec<String>, String>;
    fn download(&self, file_id: &str) -> Result<Vec<u8>, String>;
    /// Indexes the folder, then keeps refreshing it every `every` until the folder is dropped.
    /// With `strict`, a failed first index is an error; otherwise it starts empty and keeps trying.
    fn open_folder(self: Arc<Self>, folder_id: &str, folder_name: &str, every: Duration, strict: bool) -> Result<Arc<dyn DriveFolder>, String>;
}

/// A Drive folder being served.
pub trait DriveFolder: Send + Sync {
    fn folder_id(&self) -> String;
    fn folder_name(&self) -> String;
    fn client_email(&self) -> String;
    fn status(&self) -> RefreshStatus;
    fn refresh(&self) -> Result<(), String>;
    fn list_apps(&self) -> Vec<String>;
    /// Only files in the index can be read, so a request can never name an arbitrary Drive file.
    fn has(&self, app: &str, rel: &str) -> bool;
    fn read(&self, app: &str, rel: &str) -> Option<Vec<u8>>;
}
