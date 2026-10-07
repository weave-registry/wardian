//! Configuration: every environment variable Wardian reads, in one place, with its default.
//! Part of the composition root: main.rs hands these values to the adapters and use cases.

use crate::domain::splunk::SplunkConfig;
use std::{path::PathBuf, time::Duration};

/// Where the server listens unless ADDR says otherwise.
const DEFAULT_ADDR: &str = "127.0.0.1:8000";
/// The example in the message shown when the address is taken.
pub const ADDR_EXAMPLE: &str = "127.0.0.1:8001";
const DRIVE_API: &str = "https://www.googleapis.com";

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// The data folder: DATA_DIR, or ./data. The command line needs it before the rest (promote).
pub fn data_dir() -> PathBuf {
    PathBuf::from(env("DATA_DIR").unwrap_or_else(|| "data".into()))
}

/// The repository's example apps, which seed the working folder on the first start.
pub const SOURCE_APPS: &str = "apps";

pub struct Settings {
    /// The folder Wardian serves and saves apps in: the command line's, or `<data dir>/apps`.
    pub local_root: PathBuf,
    /// True when the command line named the folder; false for the working folder in the data dir.
    pub chosen_folder: bool,
    pub data_dir: PathBuf,
    pub addr: String,
    pub admin_token: Option<String>,
    /// Google Drive: the API address (GDRIVE_API_BASE, for tests), how often to refresh a served
    /// folder, a key file and a folder to serve at start.
    pub drive_api: String,
    pub refresh_every: Duration,
    pub drive_key_file: Option<String>,
    pub drive_folder: Option<String>,
    /// Link imports may reach the local network (IMPORT_ALLOW_LAN=1).
    pub import_allow_lan: bool,
    /// Anthropic: the key and workspace used when Settings has none, the API address, the model.
    pub anthropic_key: Option<String>,
    pub anthropic_workspace: Option<String>,
    pub anthropic_base: Option<String>,
    pub ai_model: Option<String>,
    /// The Splunk account used when Settings has none.
    pub splunk: Option<SplunkConfig>,
}

impl Settings {
    /// `apps_folder` comes from the command line; without one, Wardian serves `<data dir>/apps`
    /// (ADR-2610071122).
    pub fn from_env(apps_folder: Option<&str>) -> Settings {
        let secs: u64 = env("REFRESH_SECS").and_then(|s| s.parse().ok()).unwrap_or(60);
        let data = data_dir();
        Settings {
            local_root: apps_folder.map(PathBuf::from).unwrap_or_else(|| data.join("apps")),
            chosen_folder: apps_folder.is_some(),
            data_dir: data,
            addr: env("ADDR").unwrap_or_else(|| DEFAULT_ADDR.into()),
            admin_token: env("ADMIN_TOKEN"),
            drive_api: env("GDRIVE_API_BASE").unwrap_or_else(|| DRIVE_API.into()),
            refresh_every: Duration::from_secs(secs.max(5)),
            drive_key_file: env("GDRIVE_SA_KEY"),
            drive_folder: env("GDRIVE_FOLDER_ID"),
            import_allow_lan: env("IMPORT_ALLOW_LAN").is_some_and(|v| v == "1"),
            anthropic_key: env("ANTHROPIC_API_KEY"),
            anthropic_workspace: env("ANTHROPIC_WORKSPACE_ID"),
            anthropic_base: env("ANTHROPIC_BASE_URL"),
            ai_model: env("WARDIAN_AI_MODEL").or_else(|| env("RUSTLE_AI_MODEL")),
            splunk: env("SPLUNK_URL").map(|url| SplunkConfig {
                url,
                token: env("SPLUNK_TOKEN").unwrap_or_default(),
                username: env("SPLUNK_USERNAME").unwrap_or_default(),
                password: env("SPLUNK_PASSWORD").unwrap_or_default(),
                insecure_tls: env("SPLUNK_INSECURE_TLS").is_some_and(|v| v == "1" || v == "true"),
                ca_file: env("SPLUNK_CA_FILE").unwrap_or_default(),
            }),
        }
    }
}
