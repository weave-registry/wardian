//! The live source and the settings behind it. The source can be swapped
//! while the server runs, and the choice is saved to `<data dir>/config.json`.

use crate::import::{app_name_from, import_zip};
use crate::source::{unix_now, valid_drive_id, Drive, DriveClient, Source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

pub const MAX_ZIP_BYTES: u64 = 100 * 1024 * 1024;

/// Resolves a host for a link import and drops every address the server
/// should never fetch for a user: itself, cloud metadata (169.254.169.254),
/// and, unless `allow_lan`, the private network. ureq resolves through this
/// for every connection, redirects included, so a link cannot reach an
/// internal address by redirecting or by re-resolving to a new one.
fn public_only(netloc: &str, allow_lan: bool) -> std::io::Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
    let ok: Vec<SocketAddr> = addrs.iter().copied().filter(|a| fetchable(a.ip(), allow_lan)).collect();
    if ok.is_empty() && !addrs.is_empty() {
        let hint = if allow_lan { "" } else { " (set IMPORT_ALLOW_LAN=1 to allow the local network)" };
        return Err(std::io::Error::other(format!("links to internal addresses are blocked{hint}")));
    }
    Ok(ok)
}

fn fetchable(ip: IpAddr, allow_lan: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            let never = v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)); // carrier-grade NAT
            !never && (allow_lan || !v4.is_private())
        }
        IpAddr::V6(v6) => {
            // An IPv4 address written as IPv6 (::ffff:127.0.0.1) gets the IPv4 rules.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return fetchable(IpAddr::V4(v4), allow_lan);
            }
            let first = v6.segments()[0];
            let never = v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xffc0) == 0xfe80; // link-local
            let lan = (first & 0xfe00) == 0xfc00; // unique local
            !never && (allow_lan || !lan)
        }
    }
}

/// The file ID in a Drive link: .../file/d/<ID>/view or ...?id=<ID>.
fn drive_file_id(url: &str) -> Option<String> {
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

#[derive(Default, Serialize, Deserialize)]
struct Config {
    /// "local" or "drive"
    source: String,
    folder_id: Option<String>,
    folder_name: Option<String>,
}

pub struct Hub {
    data_dir: PathBuf,
    local_root: PathBuf,
    api_base: String,
    refresh_every: Duration,
    client: Mutex<Option<Arc<DriveClient>>>,
    source: RwLock<Arc<Source>>,
}

impl Hub {
    pub fn new(data_dir: PathBuf, local_root: PathBuf, api_base: String, refresh_every: Duration) -> Hub {
        Hub {
            data_dir,
            local_root: local_root.clone(),
            api_base,
            refresh_every,
            client: Mutex::new(None),
            source: RwLock::new(Arc::new(Source::Local(local_root))),
        }
    }

    fn key_path(&self) -> PathBuf {
        self.data_dir.join("service-account.json")
    }

    fn config_path(&self) -> PathBuf {
        self.data_dir.join("config.json")
    }

    /// Restores the saved state. A key uploaded through the UI wins over
    /// `env_key`; a folder in `env_folder` wins over the saved folder.
    pub fn start(&self, env_key: Option<String>, env_folder: Option<String>) {
        let key = [Some(self.key_path()), env_key.map(PathBuf::from)]
            .into_iter()
            .flatten()
            .find(|p| p.is_file());
        if let Some(path) = key {
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|raw| DriveClient::from_json(&raw, &self.api_base))
            {
                Ok(c) => *self.client.lock().unwrap() = Some(c),
                Err(e) => eprintln!("drive: key {}: {e}", path.display()),
            }
        }

        let cfg: Config = fs::read_to_string(self.config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let (folder, name) = match env_folder {
            Some(id) => (Some(id), None),
            None if cfg.source == "drive" => (cfg.folder_id, cfg.folder_name),
            None => (None, None),
        };
        if let Some(id) = folder {
            let name = name.unwrap_or_else(|| id.clone());
            match self.connect_drive(&id, &name, false) {
                Ok(()) => println!("source: google drive folder {name} ({id})"),
                Err(e) => eprintln!("drive: not connected, serving local apps: {e}"),
            }
        }
        if let Source::Local(root) = &*self.source() {
            println!("source: local dir {}", root.display());
        }
    }

    pub fn source(&self) -> Arc<Source> {
        Arc::clone(&self.source.read().unwrap())
    }

    fn client(&self) -> Result<Arc<DriveClient>, String> {
        self.client
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no service account key uploaded yet".into())
    }

    pub fn status(&self) -> Value {
        let source = self.source();
        let client_email = self.client.lock().unwrap().as_ref().map(|c| c.client_email().to_string());
        let apps = source.list_apps().len();
        match &*source {
            Source::Local(root) => json!({
                "source": "local",
                "local_root": root.display().to_string(),
                "apps": apps,
                "client_email": client_email,
            }),
            Source::Drive(d) => json!({
                "source": "drive",
                "local_root": self.local_root.display().to_string(),
                "folder_id": d.folder_id,
                "folder_name": d.folder_name,
                "apps": apps,
                "client_email": client_email,
                "drive_email": d.client().client_email(),
                "refresh_secs": self.refresh_every.as_secs(),
                "status": d.status(),
                "now": unix_now(),
            }),
        }
    }

    /// Checks a key against Google before saving it, so a bad key never
    /// replaces a good one. Returns the address to share folders with.
    pub fn set_key(&self, raw: &str) -> Result<String, String> {
        let client = DriveClient::from_json(raw, &self.api_base)?;
        client.check()?;
        write_private(&self.key_path(), raw.as_bytes())
            .map_err(|e| format!("saving key: {e}"))?;
        let email = client.client_email().to_string();
        *self.client.lock().unwrap() = Some(client);
        Ok(email)
    }

    pub fn browse(&self, parent: Option<&str>) -> Result<Value, String> {
        Ok(json!({ "folders": self.client()?.browse(parent)? }))
    }

    pub fn preview(&self, folder_id: &str) -> Result<Value, String> {
        Ok(json!({ "apps": self.client()?.preview(folder_id)? }))
    }

    pub fn refresh(&self) -> Result<(), String> {
        match &*self.source() {
            Source::Local(_) => Ok(()),
            Source::Drive(d) => d.refresh(),
        }
    }

    pub fn use_local(&self) -> Result<(), String> {
        *self.source.write().unwrap() = Arc::new(Source::Local(self.local_root.clone()));
        self.save(Config { source: "local".into(), ..Default::default() })
    }

    pub fn use_drive(&self, folder_id: &str, folder_name: &str) -> Result<(), String> {
        self.connect_drive(folder_id, folder_name, true)?;
        self.save(Config {
            source: "drive".into(),
            folder_id: Some(folder_id.into()),
            folder_name: Some(folder_name.into()),
        })
    }

    fn connect_drive(&self, folder_id: &str, folder_name: &str, strict: bool) -> Result<(), String> {
        if !valid_drive_id(folder_id) {
            return Err("folder id looks invalid".into());
        }
        let drive = Drive::connect(self.client()?, folder_id, folder_name, self.refresh_every, strict)?;
        // The old source is dropped here; if it was a Drive, its background
        // refresh notices and stops.
        *self.source.write().unwrap() = Arc::new(Source::Drive(drive));
        Ok(())
    }

    /// Unpacks a zip into the local apps folder. The apps show up once the
    /// local folder is the source; the reply says whether it is.
    pub fn import(&self, bytes: &[u8], zip_name: &str, replace: bool) -> Result<Value, String> {
        let done = import_zip(bytes, zip_name, &self.local_root, replace)?;
        println!("import: {} -> {}", zip_name, done.apps.join(", "));
        let serving_local = matches!(&*self.source(), Source::Local(_));
        Ok(json!({ "apps": done.apps, "skipped": done.skipped, "serving_local": serving_local }))
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
                let allow_lan = std::env::var("IMPORT_ALLOW_LAN").is_ok_and(|v| v == "1");
                let resp = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(60))
                    .resolver(move |netloc: &str| public_only(netloc, allow_lan))
                    .build()
                    .get(url)
                    .call()
                    .map_err(|e| format!("download failed: {e}"))?;
                let mut buf = Vec::new();
                resp.into_reader()
                    .take(MAX_ZIP_BYTES + 1)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("download failed: {e}"))?;
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

    fn save(&self, cfg: Config) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
        write_private(&self.config_path(), &body).map_err(|e| format!("saving config: {e}"))
    }
}

/// Writes through a temp file and a rename, so a crash never leaves half a
/// file, and makes the file readable by its owner only.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::fetchable;

    #[test]
    fn blocks_internal_addresses() {
        for ip in ["127.0.0.1", "169.254.169.254", "0.0.0.0", "100.64.0.1", "::1", "fe80::1", "::ffff:127.0.0.1", "::ffff:169.254.169.254"] {
            assert!(!fetchable(ip.parse().unwrap(), true), "{ip} must always be blocked");
        }
        for ip in ["10.0.0.5", "192.168.1.10", "172.16.0.1", "fd00::1"] {
            assert!(!fetchable(ip.parse().unwrap(), false), "{ip} blocked by default");
            assert!(fetchable(ip.parse().unwrap(), true), "{ip} allowed with IMPORT_ALLOW_LAN");
        }
        for ip in ["8.8.8.8", "142.250.80.46", "2607:f8b0::1"] {
            assert!(fetchable(ip.parse().unwrap(), false), "{ip} is public");
        }
    }
}
