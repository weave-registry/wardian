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

/// Who counts as an admin, said at start; or, when Wardian must not start, why (ADR-2610072033).
/// Without ADMIN_TOKEN every program on this machine is an admin. That is safe only while Wardian
/// listens on a loopback address, so any other address without a token is refused, not warned about.
pub fn admins(addr: &str, token_set: bool) -> Result<String, String> {
    if token_set {
        return Ok("admin: whoever sends ADMIN_TOKEN (Settings asks for it); without it, settings are locked from every address, this machine included".into());
    }
    if is_loopback(addr) {
        return Ok(format!("admin: every program and browser on this machine (no ADMIN_TOKEN is set, and {addr} is reachable only from here)"));
    }
    Err(format!(
        "Wardian will not listen on {addr} without ADMIN_TOKEN: other machines could reach it, and without a token \
         every program on this machine is an admin.\nSet ADMIN_TOKEN to a long random string (Settings then asks for it), \
         or listen on this machine only, e.g. ADDR={ADDR_EXAMPLE}."
    ))
}

/// Whether ADDR names a loopback address: 127.0.0.0/8, ::1 or localhost.
fn is_loopback(addr: &str) -> bool {
    let host = match addr.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((host, _)) => host,
            None => return false,
        },
        None => addr.rsplit_once(':').map_or(addr, |(host, _)| host),
    };
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
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
    /// Amazon Bedrock (ADR-2610071106): WARDIAN_AI_PROVIDER, the region and sign-in from the usual
    /// AWS variables, the models, and an address for tests.
    pub ai_provider: Option<String>,
    pub aws_region: Option<String>,
    pub bedrock_token: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_session_token: Option<String>,
    pub bedrock_base: Option<String>,
    pub bedrock_model: Option<String>,
    pub bedrock_quick_model: Option<String>,
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
            ai_provider: env("WARDIAN_AI_PROVIDER"),
            aws_region: env("AWS_REGION").or_else(|| env("AWS_DEFAULT_REGION")),
            bedrock_token: env("AWS_BEARER_TOKEN_BEDROCK"),
            aws_access_key_id: env("AWS_ACCESS_KEY_ID"),
            aws_secret_access_key: env("AWS_SECRET_ACCESS_KEY"),
            aws_session_token: env("AWS_SESSION_TOKEN"),
            bedrock_base: env("WARDIAN_BEDROCK_BASE_URL"),
            bedrock_model: env("WARDIAN_BEDROCK_MODEL"),
            bedrock_quick_model: env("WARDIAN_BEDROCK_QUICK_MODEL"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_token_only_loopback_starts() {
        for addr in ["127.0.0.1:8000", "127.0.0.1:0", "127.1.2.3:80", "localhost:8000", "LOCALHOST:1", "[::1]:8000", DEFAULT_ADDR, ADDR_EXAMPLE] {
            let who = admins(addr, false).unwrap_or_else(|e| panic!("{addr}: {e}"));
            assert!(who.contains("this machine"), "{addr}: {who}");
        }
        for addr in ["0.0.0.0:8000", "[::]:8000", "192.168.1.5:8000", "10.0.0.1:0", "example.com:80", "localhost.example.com:80", ":8000", "[::ffff:10.0.0.1]:1", "[::1:8000", ""] {
            let why = admins(addr, false).expect_err(addr);
            assert!(why.contains("set ADMIN_TOKEN") || why.contains("Set ADMIN_TOKEN"), "{addr}: {why}");
            assert!(why.contains(&format!("listen on {addr} ")), "{addr}: {why}");
        }
    }

    #[test]
    fn with_a_token_any_address_starts() {
        for addr in ["0.0.0.0:8000", "127.0.0.1:0", "[::]:80"] {
            assert!(admins(addr, true).unwrap().contains("whoever sends ADMIN_TOKEN"));
        }
    }
}
