//! Configuration: every environment variable Wardian reads, in one place, with its default.
//! Part of the composition root: main.rs hands these values to the adapters and use cases.

use crate::domain::splunk::SplunkConfig;
use crate::ports::storage::FileSystem;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Where the server listens unless ADDR says otherwise.
const DEFAULT_ADDR: &str = "127.0.0.1:8000";
/// The example in the message shown when the address is taken.
pub const ADDR_EXAMPLE: &str = "127.0.0.1:8001";
const DRIVE_API: &str = "https://www.googleapis.com";

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// The folder Wardian starts in. Empty, so the folders found there print as `data` and `apps`.
const HERE: &str = "";

/// The data folder (ADR-2610080915): DATA_DIR, or the folder rule. The command line needs it
/// before the rest (promote, export).
pub fn data_dir(fs: &dyn FileSystem) -> PathBuf {
    let platform = platform_data_dir(std::env::consts::OS, env("HOME").as_deref(), env("XDG_DATA_HOME").as_deref());
    choose_data_dir(fs, Path::new(HERE), env("DATA_DIR"), platform)
}

/// The data folder rule: DATA_DIR when set; `./data` in a Wardian checkout or when it already
/// exists (an older start outside a checkout made it, and it keeps being used); otherwise the
/// platform's folder. Without a home folder there is no platform folder, so `./data` again.
fn choose_data_dir(fs: &dyn FileSystem, here: &Path, data_env: Option<String>, platform: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = data_env {
        return PathBuf::from(dir);
    }
    let local = here.join("data");
    if is_checkout(fs, here) || fs.exists(&local) {
        return local;
    }
    platform.unwrap_or(local)
}

/// The platform's folder for an app's data: `~/Library/Application Support/Wardian` on macOS,
/// `$XDG_DATA_HOME/wardian` (default `~/.local/share/wardian`) elsewhere. The XDG rules say a
/// relative XDG_DATA_HOME is ignored.
fn platform_data_dir(os: &str, home: Option<&str>, xdg_data_home: Option<&str>) -> Option<PathBuf> {
    let home = home.filter(|h| !h.is_empty()).map(PathBuf::from);
    if os == "macos" {
        return home.map(|h| h.join("Library").join("Application Support").join("Wardian"));
    }
    let xdg = xdg_data_home.map(PathBuf::from).filter(|x| x.is_absolute());
    xdg.or_else(|| home.map(|h| h.join(".local").join("share"))).map(|d| d.join("wardian"))
}

/// Whether `here` is a Wardian checkout: a Cargo.toml naming the `wardian` package, beside `apps/`.
fn is_checkout(fs: &dyn FileSystem, here: &Path) -> bool {
    fs.is_dir(&here.join(SOURCE_APPS)) && fs.read(&here.join("Cargo.toml")).is_some_and(|b| names_wardian(&String::from_utf8_lossy(&b)))
}

/// Whether a Cargo.toml's `[package]` is named `wardian`.
fn names_wardian(toml: &str) -> bool {
    let mut in_package = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if let Some(value) = line.strip_prefix("name").map(str::trim_start).and_then(|v| v.strip_prefix('=')).filter(|_| in_package) {
            return value.trim().trim_matches('"') == "wardian";
        }
    }
    false
}

/// Where the first start copies example apps from (ADR-2610080915), in order: `./apps` in a
/// checkout; `../lib/wardian/example-apps` beside the program (the Linux layout and install.sh);
/// `../Resources/apps` beside it (the macOS app). None: the working folder starts empty.
/// Not under `share/wardian`: with the prefix ~/.local that is the Linux data folder itself.
fn example_apps(fs: &dyn FileSystem, here: &Path, program_dir: Option<&Path>) -> Option<PathBuf> {
    if is_checkout(fs, here) {
        return Some(here.join(SOURCE_APPS));
    }
    let prefix = program_dir?.parent()?;
    [prefix.join("lib").join("wardian").join(SHARED_EXAMPLE_APPS), prefix.join("Resources").join(SOURCE_APPS)].into_iter().find(|p| fs.is_dir(p))
}

/// The example apps' folder name under `<prefix>/lib/wardian` (release tarballs, Linux package).
const SHARED_EXAMPLE_APPS: &str = "example-apps";

/// The folder this program is in, symbolic links followed, so a link in another folder still
/// finds the example apps installed beside the real file.
fn program_dir(fs: &dyn FileSystem) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    fs.canonical(&exe).unwrap_or(exe).parent().map(Path::to_path_buf)
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
const SOURCE_APPS: &str = "apps";

pub struct Settings {
    /// The folder Wardian serves and saves apps in: the command line's, or `<data dir>/apps`.
    pub local_root: PathBuf,
    /// True when the command line named the folder; false for the working folder in the data dir.
    pub chosen_folder: bool,
    pub data_dir: PathBuf,
    /// Where the first start copies example apps from, if anywhere (ADR-2610080915).
    pub example_apps: Option<PathBuf>,
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
    pub fn from_env(apps_folder: Option<&str>, fs: &dyn FileSystem) -> Settings {
        let secs: u64 = env("REFRESH_SECS").and_then(|s| s.parse().ok()).unwrap_or(60);
        let data = data_dir(fs);
        Settings {
            example_apps: example_apps(fs, Path::new(HERE), program_dir(fs).as_deref()),
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
    use crate::adapters::secondary::local_disk::LocalDisk;
    use std::fs;

    /// A fresh empty folder under the system's temporary folder.
    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wardian-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Makes `dir` look like a checkout of `package`: a Cargo.toml and an apps folder.
    fn checkout(dir: &Path, package: &str) {
        fs::create_dir_all(dir.join("apps")).unwrap();
        fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"{package}\"\nversion = \"1.0.0\"\n\n[dependencies]\nname = \"x\"\n")).unwrap();
    }

    #[test]
    fn data_folder_is_data_dir_then_checkout_then_existing_then_platform() {
        let platform = Some(PathBuf::from("/platform/wardian"));
        let here = tmp("data-rule");
        // An unrelated empty folder: the platform's folder.
        assert_eq!(choose_data_dir(&LocalDisk, &here, None, platform.clone()), PathBuf::from("/platform/wardian"));
        // No home folder, so no platform folder: ./data.
        assert_eq!(choose_data_dir(&LocalDisk, &here, None, None), here.join("data"));
        // DATA_DIR always wins.
        assert_eq!(choose_data_dir(&LocalDisk, &here, Some("/set/by/env".into()), platform.clone()), PathBuf::from("/set/by/env"));
        // A Cargo.toml for another package beside apps/ is not a checkout.
        checkout(&here, "something-else");
        assert_eq!(choose_data_dir(&LocalDisk, &here, None, platform.clone()), PathBuf::from("/platform/wardian"));
        // A Wardian checkout: ./data, as before.
        checkout(&here, "wardian");
        assert_eq!(choose_data_dir(&LocalDisk, &here, None, platform.clone()), here.join("data"));
        assert_eq!(choose_data_dir(&LocalDisk, &here, Some("elsewhere".into()), platform.clone()), PathBuf::from("elsewhere"));
        // Cargo.toml without apps/ beside it is not a checkout.
        fs::remove_dir_all(here.join("apps")).unwrap();
        assert_eq!(choose_data_dir(&LocalDisk, &here, None, platform.clone()), PathBuf::from("/platform/wardian"));
        // A ./data an older start made outside a checkout keeps being used.
        let older = tmp("data-older");
        fs::create_dir_all(older.join("data")).unwrap();
        assert_eq!(choose_data_dir(&LocalDisk, &older, None, platform), older.join("data"));
        let _ = fs::remove_dir_all(here);
        let _ = fs::remove_dir_all(older);
    }

    #[test]
    fn platform_folder_per_os() {
        assert_eq!(platform_data_dir("macos", Some("/u/a"), Some("/x")), Some(PathBuf::from("/u/a/Library/Application Support/Wardian")));
        assert_eq!(platform_data_dir("linux", Some("/u/a"), None), Some(PathBuf::from("/u/a/.local/share/wardian")));
        assert_eq!(platform_data_dir("linux", Some("/u/a"), Some("/xdg")), Some(PathBuf::from("/xdg/wardian")));
        assert_eq!(platform_data_dir("linux", Some("/u/a"), Some("relative")), Some(PathBuf::from("/u/a/.local/share/wardian")));
        assert_eq!(platform_data_dir("linux", None, Some("/xdg")), Some(PathBuf::from("/xdg/wardian")));
        assert_eq!(platform_data_dir("linux", None, None), None);
        assert_eq!(platform_data_dir("macos", Some(""), None), None);
    }

    #[test]
    fn checkout_reads_the_package_name_only() {
        assert!(names_wardian("[package]\nname = \"wardian\"\n"));
        assert!(names_wardian("# x\n[package]\nversion = \"1\"\nname=\"wardian\"\n"));
        assert!(!names_wardian("[package]\nname = \"wardian-tools\"\n"));
        assert!(!names_wardian("[dependencies]\nname = \"wardian\"\n"));
        assert!(!names_wardian("[package]\nnamespace = \"wardian\"\n"));
        assert!(!names_wardian(""));
        // The repository these tests run in is a checkout.
        assert!(is_checkout(&LocalDisk, Path::new(env!("CARGO_MANIFEST_DIR"))));
    }

    #[test]
    fn example_apps_from_checkout_then_linux_layout_then_macos_app() {
        let root = tmp("example-apps");
        let here = root.join("somewhere");
        fs::create_dir_all(&here).unwrap();
        // Linux layout: <prefix>/bin/wardian and <prefix>/lib/wardian/example-apps; never its apps folder.
        let linux = root.join("prefix");
        fs::create_dir_all(linux.join("bin")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, Some(&linux.join("bin"))), None);
        fs::create_dir_all(linux.join("share/wardian/apps")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, Some(&linux.join("bin"))), None);
        fs::create_dir_all(linux.join("lib/wardian/example-apps")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, Some(&linux.join("bin"))), Some(linux.join("lib/wardian/example-apps")));
        // macOS app: Contents/MacOS/wardian and Contents/Resources/apps.
        let mac = root.join("Wardian.app/Contents");
        fs::create_dir_all(mac.join("MacOS")).unwrap();
        fs::create_dir_all(mac.join("Resources/apps")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, Some(&mac.join("MacOS"))), Some(mac.join("Resources/apps")));
        // A plain ./apps outside a checkout is not taken; in a checkout it comes first.
        fs::create_dir_all(here.join("apps")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, None), None);
        checkout(&here, "wardian");
        assert_eq!(example_apps(&LocalDisk, &here, Some(&linux.join("bin"))), Some(here.join("apps")));
        let _ = fs::remove_dir_all(root);
    }

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
