//! Configuration: every environment variable Wardian reads, in one place, with its default.
//! Part of the composition root: main.rs hands these values to the adapters and use cases.

use crate::domain::splunk::SplunkConfig;
use crate::usecases::aws_profiles::AwsEnv;
use crate::ports::storage::FileSystem;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Where the server listens unless ADDR says otherwise.
const DEFAULT_ADDR: &str = "127.0.0.1:8000";
/// The example in the message shown when the address is taken.
const ADDR_EXAMPLE: &str = "127.0.0.1:8001";
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
    // A build run from elsewhere: <checkout>/target/<profile>/wardian (or a deps/ test binary)
    // finds the apps of the checkout it was built in.
    if let Some(checkout) = program_dir?.ancestors().skip(1).take(3).find(|d| d.file_name().is_some() && is_checkout(fs, d)) {
        return Some(checkout.join(SOURCE_APPS));
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
        return Ok("admin: whoever sends the admin token (ADMIN_TOKEN, or the one saved in Settings → Keys; Settings asks for it); without it, settings are locked from every address, this machine included".into());
    }
    if is_loopback(addr) {
        return Ok(format!("admin: every program and browser on this machine (no ADMIN_TOKEN is set, and {addr} is reachable only from here)"));
    }
    Err(format!(
        "Wardian will not listen on {addr} without ADMIN_TOKEN: other machines could reach it, and without a token \
         every program on this machine is an admin.\nSet ADMIN_TOKEN to a long random string (Settings then asks for it), \
         save a token in Settings → Keys first, or listen on this machine only, e.g. ADDR={ADDR_EXAMPLE}."
    ))
}

/// Who counts as an admin, in the few words the terminal start block shows (ADR-2610080930).
/// Only a loopback address starts without a token, so "this computer only" is exact.
pub fn admins_in_short(token_set: bool) -> &'static str {
    if token_set {
        "whoever sends the admin token"
    } else {
        "this computer only"
    }
}

/// When ADDR is not set and the usual port is taken by another program, the ports after it are
/// tried up to this one (ADR-2610080930).
pub const LAST_PORT: u16 = 8010;

/// What a port turned out to be when Wardian tried to take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Free, and now Wardian's.
    Free,
    /// Taken by this same Wardian: the same version, serving the same apps folder.
    Wardian,
    /// Taken by another Wardian: an older or newer version, or one serving another folder.
    OtherWardian,
    /// Taken by something else.
    Other,
}

/// The outcome of the port rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortChoice {
    /// Listen on this port.
    Use(u16),
    /// A Wardian already runs on this port: open it instead.
    Running(u16),
    /// Every port in the range is held by other programs.
    NoneFree,
}

/// The port rule, when ADDR is not set (ADR-2610080930): the ports from `first` to `last` in
/// order; the first free one is used, unless this same Wardian is found first, which is then
/// opened. Another Wardian is passed over like any other program.
/// `ask` tries a port and says what it is; it is asked about each port at most once, in order.
pub fn choose_port(first: u16, last: u16, mut ask: impl FnMut(u16) -> Port) -> PortChoice {
    for port in first..=last {
        match ask(port) {
            Port::Free => return PortChoice::Use(port),
            Port::Wardian => return PortChoice::Running(port),
            Port::OtherWardian | Port::Other => {}
        }
    }
    PortChoice::NoneFree
}

/// Whether a Wardian found on a port is this one: the same version, serving the same apps folder.
/// One that reports no version is older than any that does.
pub fn same_wardian(found_version: Option<&str>, found_root: &str, version: &str, root: &str) -> bool {
    found_version == Some(version) && found_root == root
}

/// What the start block says about another Wardian on the usual port, and how to stop it.
pub fn other_wardian_note(port: u16, found_version: Option<&str>, found_root: &str) -> String {
    let which = found_version.map_or_else(|| "an older Wardian".to_string(), |v| format!("Wardian {v}"));
    format!("{which} serving {found_root} holds port {port}. Stop it (Ctrl-C where it runs) to use {port}.")
}

/// `host:port` split at the last colon, for the port rule. None when there is no number.
pub fn host_and_port(addr: &str) -> Option<(&str, u16)> {
    let (host, port) = addr.rsplit_once(':')?;
    Some((host, port.parse().ok()?))
}

/// Why Wardian cannot listen on `addr`, and what to do, as two sentences. `error` is the system's
/// reason, or None when the address is in use.
pub fn cannot_listen(addr: &str, error: Option<&str>, tried_up_to: Option<u16>) -> (String, String) {
    match (tried_up_to, error) {
        (Some(last), _) => (
            format!("Wardian cannot listen on {addr} or the ports after it up to {last}: other programs are using them."),
            format!("Stop one of them, or choose a port with ADDR, e.g. ADDR={}:{} wardian", host_and_port(addr).map_or("127.0.0.1", |(h, _)| h), last.saturating_add(10)),
        ),
        (None, None) => (
            format!("Wardian cannot listen on {addr}: another program (perhaps another Wardian) is using that address."),
            format!("Stop it, or pick another port, e.g. ADDR={ADDR_EXAMPLE} wardian"),
        ),
        (None, Some(e)) => (format!("Wardian cannot listen on {addr}: {e}."), format!("Pick another address, e.g. ADDR={ADDR_EXAMPLE} wardian")),
    }
}

/// Whether ADDR names a loopback address: 127.0.0.0/8, ::1 or localhost.
pub fn is_loopback(addr: &str) -> bool {
    let host = match addr.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((host, _)) => host,
            None => return false,
        },
        None => addr.rsplit_once(':').map_or(addr, |(host, _)| host),
    };
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The user's config folder: XDG_CONFIG_HOME, or `~/.config`.
pub fn config_home() -> Option<PathBuf> {
    env("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// The service's launchd label (ADR-2610081800); the systemd unit is named after it. Tests set
/// WARDIAN_SERVICE_LABEL to a label of their own, so they never touch the user's service.
const SERVICE_LABEL: &str = "studio.wardian";

/// WARDIAN_SERVICE_LABEL, or `studio.wardian`. Err for a label that is not letters, digits, dots
/// and dashes starting with `studio.wardian`.
pub fn service_label() -> Result<String, String> {
    match env("WARDIAN_SERVICE_LABEL") {
        None => Ok(SERVICE_LABEL.into()),
        Some(l) if valid_label(&l) => Ok(l),
        Some(l) => Err(format!("WARDIAN_SERVICE_LABEL={l} is not a label Wardian uses: it must start with {SERVICE_LABEL} and hold only letters, digits, dots and dashes")),
    }
}

fn valid_label(l: &str) -> bool {
    l.starts_with(SERVICE_LABEL) && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The service's variables beside DATA_DIR and WARDIAN_NO_OPEN (ADR-2610081800): the address, the
/// home folder (which places the master key and `~/.aws`), where the master key is, and where the
/// AWS config and credentials files are (ADR-2610091530), when set. Never a secret.
pub fn service_env() -> Vec<(String, String)> {
    ["ADDR", "HOME", "XDG_CONFIG_HOME", "WARDIAN_MASTER_KEY_FILE", "AWS_CONFIG_FILE", "AWS_SHARED_CREDENTIALS_FILE"].iter().filter_map(|k| env(k).map(|v| (k.to_string(), v))).collect()
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
    /// True when ADDR was set: then a busy port is an error, not a reason to try the next one.
    pub addr_set: bool,
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
    /// An AWS profile (ADR-2610091530): AWS_PROFILE, the files AWS_CONFIG_FILE and
    /// AWS_SHARED_CREDENTIALS_FILE name, WARDIAN_AWS_CLI, and PATH and HOME to find the rest.
    pub aws: AwsEnv,
    pub aws_profile: Option<String>,
    pub bedrock_base: Option<String>,
    pub bedrock_model: Option<String>,
    pub bedrock_quick_model: Option<String>,
    /// The Splunk account used when Settings has none.
    pub splunk: Option<SplunkConfig>,
    /// Where the master key that seals secrets is kept (ADR-2610081501): WARDIAN_MASTER_KEY,
    /// WARDIAN_MASTER_KEY_FILE, or else the key file in the user's folder.
    pub master_key: Option<String>,
    pub master_key_file: Option<PathBuf>,
    pub user_key_file: Option<PathBuf>,
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
            addr_set: env("ADDR").is_some(),
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
            aws: AwsEnv {
                home: env("HOME").map(PathBuf::from),
                config_file: env("AWS_CONFIG_FILE").map(PathBuf::from),
                credentials_file: env("AWS_SHARED_CREDENTIALS_FILE").map(PathBuf::from),
                path: env("PATH"),
                cli: env("WARDIAN_AWS_CLI"),
            },
            aws_profile: env("AWS_PROFILE"),
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
            master_key: env("WARDIAN_MASTER_KEY"),
            master_key_file: env("WARDIAN_MASTER_KEY_FILE").map(PathBuf::from),
            user_key_file: config_home().map(|d| d.join("wardian").join("master.key")),
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
    fn service_label_is_wardians_own() {
        assert!(valid_label("studio.wardian"));
        assert!(valid_label("studio.wardian.test-1a2b"));
        assert!(!valid_label("com.apple.Finder"));
        assert!(!valid_label("studio.wardian/../x"));
        assert!(!valid_label("studio.wardian x"));
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
        // A build in a checkout's target/ folder, started from another folder, finds that checkout's apps.
        let repo = root.join("repo");
        checkout(&repo, "wardian");
        fs::create_dir_all(repo.join("target/release/deps")).unwrap();
        assert_eq!(example_apps(&LocalDisk, &here, Some(&repo.join("target/release"))), Some(repo.join("apps")));
        assert_eq!(example_apps(&LocalDisk, &here, Some(&repo.join("target/release/deps"))), Some(repo.join("apps")));
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

    /// The port rule over a fixed set of answers, with the ports asked about in order.
    fn rule(answers: &[(u16, Port)]) -> (PortChoice, Vec<u16>) {
        let mut asked = Vec::new();
        let choice = choose_port(8000, 8003, |p| {
            asked.push(p);
            answers.iter().find(|(q, _)| *q == p).map_or(Port::Free, |(_, a)| *a)
        });
        (choice, asked)
    }

    #[test]
    fn start_port_rule_uses_the_first_free_port_or_opens_a_running_wardian() {
        assert_eq!(rule(&[]), (PortChoice::Use(8000), vec![8000]));
        // This same Wardian on the usual port: open it, try nothing else.
        assert_eq!(rule(&[(8000, Port::Wardian)]), (PortChoice::Running(8000), vec![8000]));
        // Another Wardian (older, or serving another folder): leave it, take the next free port.
        assert_eq!(rule(&[(8000, Port::OtherWardian)]), (PortChoice::Use(8001), vec![8000, 8001]));
        // Something else holds it: the next free port.
        assert_eq!(rule(&[(8000, Port::Other)]), (PortChoice::Use(8001), vec![8000, 8001]));
        assert_eq!(rule(&[(8000, Port::Other), (8001, Port::Other)]), (PortChoice::Use(8002), vec![8000, 8001, 8002]));
        // A Wardian that moved up a port earlier is found again.
        assert_eq!(rule(&[(8000, Port::Other), (8001, Port::Wardian)]), (PortChoice::Running(8001), vec![8000, 8001]));
        let all: Vec<(u16, Port)> = (8000..=8003).map(|p| (p, Port::Other)).collect();
        assert_eq!(rule(&all), (PortChoice::NoneFree, vec![8000, 8001, 8002, 8003]));
        assert_eq!(choose_port(u16::MAX, u16::MAX, |_| Port::Other), PortChoice::NoneFree);
    }

    #[test]
    fn start_opens_only_the_same_wardian() {
        assert!(same_wardian(Some("0.4.3"), "/w/apps", "0.4.3", "/w/apps"));
        assert!(!same_wardian(Some("0.4.2"), "/w/apps", "0.4.3", "/w/apps"), "another version");
        assert!(!same_wardian(None, "/w/apps", "0.4.3", "/w/apps"), "a Wardian too old to say its version");
        assert!(!same_wardian(Some("0.4.3"), "/elsewhere/apps", "0.4.3", "/w/apps"), "another apps folder");
        assert_eq!(other_wardian_note(8000, None, "data/apps"), "an older Wardian serving data/apps holds port 8000. Stop it (Ctrl-C where it runs) to use 8000.");
        assert_eq!(other_wardian_note(8000, Some("0.4.2"), "/w/apps"), "Wardian 0.4.2 serving /w/apps holds port 8000. Stop it (Ctrl-C where it runs) to use 8000.");
    }

    #[test]
    fn start_address_parts_and_messages() {
        assert_eq!(host_and_port(DEFAULT_ADDR), Some(("127.0.0.1", 8000)));
        assert_eq!(host_and_port("[::1]:8000"), Some(("[::1]", 8000)));
        assert_eq!(host_and_port("localhost"), None);
        assert_eq!(host_and_port("x:port"), None);
        let (what, todo) = cannot_listen("127.0.0.1:9000", None, None);
        assert_eq!(what, "Wardian cannot listen on 127.0.0.1:9000: another program (perhaps another Wardian) is using that address.");
        assert!(todo.contains(&format!("ADDR={ADDR_EXAMPLE} wardian")), "{todo}");
        let (what, _) = cannot_listen("127.0.0.1:80", Some("Permission denied (os error 13)"), None);
        assert_eq!(what, "Wardian cannot listen on 127.0.0.1:80: Permission denied (os error 13).");
        let (what, todo) = cannot_listen(DEFAULT_ADDR, None, Some(LAST_PORT));
        assert!(what.contains("up to 8010"), "{what}");
        assert!(todo.contains("ADDR=127.0.0.1:8020 wardian"), "{todo}");
        assert_eq!(admins_in_short(false), "this computer only");
    }

    #[test]
    fn with_a_token_any_address_starts() {
        for addr in ["0.0.0.0:8000", "127.0.0.1:0", "[::]:80"] {
            assert!(admins(addr, true).unwrap().contains("whoever sends the admin token"));
        }
    }
}
