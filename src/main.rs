//! The composition root: the one place that knows every adapter. It reads the configuration,
//! builds the adapters, hands them to the use cases as ports, and starts the command line or
//! the web server. Everything else depends inward: adapters on ports, ports and use cases on
//! the domain (see .hexa/ and `hexa analyze .`).

mod config;
#[cfg(test)]
mod tests;

mod domain {
    pub mod agent;
    pub mod check;
    pub mod components;
    pub mod db;
    pub mod export;
    pub mod grants;
    pub mod history;
    pub mod import_plan;
    pub mod jobs;
    pub mod package;
    pub mod splunk;
    pub mod studio;
    pub mod suite;
    pub mod usage;
    pub mod viewer_state;
}
mod ports {
    pub mod assets;
    pub mod calendar;
    pub mod db;
    pub mod drive;
    pub mod llm;
    pub mod secrets;
    pub mod service;
    pub mod splunk;
    pub mod storage;
    pub mod tools;
    pub mod web;
}
mod usecases {
    pub mod catalog;
    pub mod check;
    pub mod db;
    pub mod docs;
    pub mod export;
    pub mod history;
    pub mod import;
    pub mod jobs;
    pub mod keys;
    pub mod scaffold;
    pub mod skills;
    pub mod splunk;
    pub mod studio;
    pub mod usage;
    pub mod viewer_state;
    pub mod workspace;
}
mod adapters {
    pub mod primary {
        pub mod cli;
        pub mod http;
        mod http_server;
        pub mod stop_log;
        pub mod terminal;
    }
    pub mod secondary {
        pub mod anthropic_inference;
        pub mod bedrock_inference;
        pub mod embedded_assets;
        pub mod google_drive;
        pub mod link_fetch;
        pub mod local_disk;
        pub mod plain_secrets;
        pub mod splunk_rest;
        pub mod sqlite_store;
        pub mod wardian_probe;
    }
}

use adapters::primary::{cli, http, stop_log::StopLog, terminal};
use adapters::secondary::{anthropic_inference, bedrock_inference::Bedrock, embedded_assets::Embedded, google_drive::GoogleDrive, link_fetch::LinkFetcher, local_disk::LocalDisk, plain_secrets::PlainSecrets, splunk_rest::SplunkRest, sqlite_store::SqliteStore, wardian_probe};
use config::Settings;
use ports::{assets::Assets, db::Database, llm::BedrockAuth, secrets::Secrets, service::{Builder, Exports, Searches, Services, ViewerState}, storage::FileSystem};
use std::sync::Arc;
use usecases::{
    catalog::{Hub, HubPorts},
    check::Checker,
    db::Db,
    docs::Docs,
    export::Exporter,
    history::History,
    scaffold::Scaffold,
    jobs::JobRunner,
    keys::{AdminGate, KeyChecks, KeyOwner, Keyring},
    splunk::Splunk,
    studio::{BedrockSettings, Providers, Stores, Studio},
    usage::Meter,
    viewer_state::State,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let assets: Arc<dyn Assets> = Arc::new(Embedded);
    let checker = Arc::new(Checker::new(Arc::clone(&fs)));
    let tools = Scaffold::new(Arc::clone(&fs), Arc::clone(&assets), Arc::clone(&checker));
    // `wardian export` reads the working folder and the app's data, so it gets the same parts
    // the server would use, built only when that command runs.
    let exports_for_cli = || -> Arc<dyn Exports> {
        let data_dir = config::data_dir(&*fs);
        let apps = data_dir.join("apps");
        let state: Arc<dyn ViewerState> = Arc::new(State::new(Arc::clone(&fs), &data_dir));
        let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&data_dir));
        let history = Arc::new(History::new(Arc::clone(&fs), &data_dir, &apps));
        Arc::new(Exporter::new(Arc::clone(&fs), Arc::clone(&checker), db, state, history, &apps, &data_dir))
    };
    let docs = Docs::new(Arc::clone(&assets));
    let (apps_folder, no_open) = match cli::run(&args, &tools, &docs, &config::data_dir(&*fs), &exports_for_cli) {
        cli::Command::Exit(code) => std::process::exit(code),
        cli::Command::Serve { folder, no_open } => (folder, no_open),
    };

    serve(Settings::from_env(apps_folder.as_deref(), &*fs), no_open);
}

/// Where the server listens, decided before anything else starts (ADR-2610080930).
enum Address {
    /// Listening; with the usual port when that was busy and a later one was taken.
    Ready(http::Listener, Option<u16>),
    /// A Wardian already answers at this host:port.
    Running(String),
    /// Nothing could be taken: what went wrong, and what to do.
    Failed(String, String),
}

/// Takes `addr`. When ADDR was not set (`addr_set` false), a busy port is not the end: a Wardian
/// answering there is reported as running, and a port held by anything else moves Wardian to the
/// next free one, up to `last_port`. With ADDR set, a busy port is an error.
fn take_address(addr: &str, addr_set: bool, last_port: u16) -> Address {
    match config::host_and_port(addr).filter(|&(_, port)| !addr_set && port != 0) {
        Some((host, first)) => {
            let (mut taken, mut why) = (None, None);
            let choice = config::choose_port(first, last_port.max(first), |port| {
                let at = format!("{host}:{port}");
                match http::listen(&at) {
                    Ok(listener) => {
                        taken = Some(listener);
                        config::Port::Free
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::AddrInUse {
                            why.get_or_insert_with(|| e.to_string());
                        }
                        if wardian_probe::is_wardian(&at) {
                            config::Port::Wardian
                        } else {
                            config::Port::Other
                        }
                    }
                }
            });
            match (choice, taken) {
                (config::PortChoice::Use(port), Some(listener)) => Address::Ready(listener, (port != first).then_some(first)),
                (config::PortChoice::Running(port), _) => Address::Running(format!("{host}:{port}")),
                _ => {
                    let (what, todo) = config::cannot_listen(addr, why.as_deref(), why.is_none().then_some(last_port.max(first)));
                    Address::Failed(what, todo)
                }
            }
        }
        None => match http::listen(addr) {
            Ok(listener) => Address::Ready(listener, None),
            Err(e) => {
                let error = (e.kind() != std::io::ErrorKind::AddrInUse).then(|| e.to_string());
                let (what, todo) = config::cannot_listen(addr, error.as_deref(), None);
                Address::Failed(what, todo)
            }
        },
    }
}

/// Builds the adapters and use cases for these settings and serves them; it does not return. The
/// secrets test starts the same server in a child process.
///
/// In a terminal (ADR-2610080930) it prints a short block once it listens and keeps the detail
/// lines in `<data dir>/wardian.log`; otherwise it prints the detail lines as it always has, for
/// the scripts, launchers and tests that read them. Errors go to stderr either way.
fn serve(cfg: Settings, no_open: bool) {
    use std::io::{IsTerminal, Write};
    let tty = std::io::stdout().is_terminal();
    let colour = terminal::Colour::from_env(tty);
    let open = terminal::should_open(tty, no_open, std::env::var("WARDIAN_NO_OPEN").ok().as_deref());
    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let assets: Arc<dyn Assets> = Arc::new(Embedded);
    let checker = Arc::new(Checker::new(Arc::clone(&fs)));
    let secrets: Arc<dyn Secrets> = Arc::new(PlainSecrets::new(Arc::clone(&fs)));
    // The admin token: ADMIN_TOKEN, or the one saved in Settings (ADR-2610081500). One that is
    // saved but cannot be read stops Wardian, so it never starts unlocked by mistake.
    let admin = match AdminGate::load(Arc::clone(&secrets), &cfg.data_dir, cfg.admin_token.clone(), !config::is_loopback(&cfg.addr)) {
        Ok(gate) => Arc::new(gate),
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let token_set = admin.token().is_some();
    // Before anything else: an address other machines can reach needs an admin token (ADR-2610072033).
    let who = match config::admins(&cfg.addr, token_set) {
        Ok(who) => who,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    // Then the address, so a Wardian already running is opened before anything is changed.
    let address = match take_address(&cfg.addr, cfg.addr_set, config::LAST_PORT) {
        Address::Running(at) => {
            let url = format!("http://{at}");
            if tty {
                let opened = open && terminal::open_browser(&url);
                println!("{}", terminal::already_running(&url, opened, colour));
            } else {
                println!("wardian: a Wardian is already running at {url}, so this one stops");
            }
            std::process::exit(0);
        }
        Address::Ready(listener, busy) => Ok((listener, busy)),
        Address::Failed(what, todo) => Err((what, todo)),
    };
    // The data folder may be relative to where Wardian starts, so name it in full, and stop at
    // once if it cannot be written: everything Wardian keeps goes there.
    let data_shown = std::env::current_dir().map(|d| d.join(&cfg.data_dir)).unwrap_or_else(|_| cfg.data_dir.clone());
    if let Err(e) = usecases::workspace::check_writable(&*fs, &cfg.data_dir) {
        eprintln!("Wardian cannot write its data folder, {}: {e}", data_shown.display());
        if cfg!(target_os = "macos") && e.contains("os error 1") {
            eprintln!("macOS blocked it. Allow your terminal app in System Settings → Privacy & Security → Files and Folders (Removable Volumes for an outside drive), or start Wardian from another folder.");
        }
        eprintln!("Wardian keeps its data in DATA_DIR if set; otherwise in ./data in a Wardian checkout or where ./data already exists; otherwise in your user data folder.");
        std::process::exit(2);
    }
    // From here on Wardian is a server, and every way it stops is written down (ADR-2610072033).
    let stops = if tty { StopLog::new(&cfg.data_dir).quiet() } else { StopLog::new(&cfg.data_dir) };
    // The detail lines: printed when the output is not a terminal, else kept in the log only.
    let detail = |line: &str| if tty { stops.note(line) } else { println!("{line}") };
    detail(&who);
    detail(&format!("data: {}", data_shown.display()));
    // A first start with an empty data folder shows the first-run setup once (ADR-2610072033).
    if let Err(e) = usecases::workspace::mark_first_run(&*fs, &cfg.data_dir) {
        eprintln!("data: could not prepare {}: {e}", cfg.data_dir.display());
    }
    stops.catch_panics();
    stops.catch_signals();
    let (listener, busy_port) = match address {
        Ok(taken) => taken,
        Err((what, todo)) => {
            stops.started(&format!("serving {} on {}", cfg.local_root.display(), cfg.addr));
            stops.record(&format!("stopped: {what} {todo}"));
            if tty {
                let stderr_colour = terminal::Colour::from_env(std::io::stderr().is_terminal());
                eprintln!("{}", terminal::error_block(&what, &todo, stderr_colour));
            }
            std::process::exit(1);
        }
    };
    let bound = listener.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| cfg.addr.clone());
    stops.started(&format!("serving {} on {bound}", cfg.local_root.display()));
    // The working folder (ADR-2610071122): filled from the example apps on the first start (found
    // by ADR-2610080915's rule), which are never changed. A folder named on the command line is
    // served as it is.
    let (mut added, mut git_note) = (None, None);
    if cfg.chosen_folder {
        if usecases::workspace::inside_git(&*fs, &cfg.local_root) {
            detail(&format!("note: {} is inside a git repository, so apps changed in Wardian show up there as uncommitted changes. Run without a folder to use {}.", cfg.local_root.display(), cfg.data_dir.join("apps").display()));
            git_note = Some("inside a git repository: changes show up there");
        }
    } else {
        // Every example app the working folder has not been given yet (ADR-2610081600): from the
        // copy on disk when there is one, else from the copies built into the program.
        let from = cfg.example_apps.as_ref().map(|s| s.display().to_string()).unwrap_or_else(|| "the copies built into Wardian".into());
        match usecases::workspace::add_examples(&*fs, cfg.example_apps.as_deref(), assets.example_apps(), &cfg.local_root, &cfg.data_dir) {
            Ok(names) if !names.is_empty() => {
                added = Some(names.len());
                detail(&format!("apps: added {} example app(s) from {from} into {} (the working folder): {}", names.len(), cfg.local_root.display(), names.join(", ")));
            }
            Ok(_) => {}
            Err(e) => eprintln!("apps: could not add the example apps to {} from {from}: {e}", cfg.local_root.display()),
        }
    }
    let history = Arc::new(History::new(Arc::clone(&fs), &cfg.data_dir, &cfg.local_root));
    let checks = Arc::new(KeyChecks::new(Arc::clone(&fs), &cfg.data_dir));
    let hub = Arc::new(Hub::new(
        HubPorts {
            fs: Arc::clone(&fs),
            drive: Arc::new(GoogleDrive::new(&cfg.drive_api)),
            web: Arc::new(LinkFetcher::new(cfg.import_allow_lan)),
            assets: Arc::clone(&assets),
            secrets: Arc::clone(&secrets),
        },
        Arc::clone(&checks),
        Arc::clone(&history),
        cfg.data_dir.clone(),
        cfg.local_root.clone(),
        cfg.refresh_every,
    ));
    let providers = Providers {
        anthropic: Arc::new(anthropic_inference::Anthropic::new(cfg.anthropic_base.as_deref().unwrap_or(anthropic_inference::DEFAULT_BASE), cfg.ai_model.clone())),
        bedrock: Arc::new(Bedrock::new(cfg.bedrock_base.clone(), cfg.bedrock_model.clone(), cfg.bedrock_quick_model.clone())),
        anthropic_key: cfg.anthropic_key.clone(),
        anthropic_workspace: cfg.anthropic_workspace.clone(),
        provider: cfg.ai_provider.clone(),
        bedrock_env: bedrock_from_env(&cfg),
    };
    let stores = Stores { fs: Arc::clone(&fs), secrets: Arc::clone(&secrets), checks: Arc::clone(&checks), meter: Arc::new(Meter::new(Arc::clone(&fs), &cfg.data_dir)) };
    let studio = Arc::new(Studio::new(stores, providers, Arc::clone(&assets), Arc::clone(&hub), Arc::clone(&checker), &cfg.data_dir));
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&cfg.data_dir));
    let splunk = Arc::new(Splunk::new(Arc::clone(&secrets), Arc::clone(&checks), Arc::new(SplunkRest), Arc::clone(&db), &cfg.data_dir, cfg.splunk.clone()));
    detail(&hub.start(cfg.drive_key_file.clone(), cfg.drive_folder.clone()));

    let state: Arc<dyn ViewerState> = Arc::new(State::new(Arc::clone(&fs), &cfg.data_dir));
    let exports = Arc::new(Exporter::new(Arc::clone(&fs), Arc::clone(&checker), Arc::clone(&db), Arc::clone(&state), Arc::clone(&history), &cfg.local_root, &cfg.data_dir));
    let owners: Vec<Arc<dyn KeyOwner>> = vec![studio.clone(), splunk.clone(), hub.clone()];
    let keys = Arc::new(Keyring::new(owners, admin, checks, secrets));
    let (builder, searches): (Arc<dyn Builder>, Arc<dyn Searches>) = (studio, splunk);
    let jobs = Arc::new(JobRunner::new(Arc::clone(&searches), Arc::clone(&builder)));
    detail(JobRunner::NOTE);
    let services = Services { exports, tables: Arc::new(Db::new(db)), history, state, catalog: hub, builder, searches, jobs, pages: Arc::new(Docs::new(assets)), keys };
    // The address actually taken: with port 0 the system picks a free port.
    let url = format!("http://{bound}");
    detail(&format!("listening on {url}"));
    if tty {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let apps_shown = std::env::current_dir().map(|d| d.join(&cfg.local_root)).unwrap_or_else(|_| cfg.local_root.clone());
        let start = terminal::Start {
            version: env!("CARGO_PKG_VERSION"),
            url: &url,
            busy_port,
            apps: &terminal::tilde(&apps_shown, home.as_deref()),
            added,
            admin: config::admins_in_short(token_set),
            log: &terminal::tilde(&data_shown.join("wardian.log"), home.as_deref()),
            note: git_note,
        };
        print!("{}", terminal::start_block(&start, colour));
        let _ = std::io::stdout().flush();
        if open {
            terminal::open_browser(&url);
        }
    }
    let why = http::serve(listener, services);
    stops.record(&format!("stopped: {why}"));
    std::process::exit(1);
}

/// Bedrock from the usual AWS variables: a region, and a Bedrock API key or access keys.
fn bedrock_from_env(cfg: &Settings) -> Option<BedrockSettings> {
    let region = cfg.aws_region.clone()?;
    let auth = match (&cfg.bedrock_token, &cfg.aws_access_key_id, &cfg.aws_secret_access_key) {
        (Some(t), _, _) => BedrockAuth::ApiKey(t.clone()),
        (None, Some(id), Some(secret)) => BedrockAuth::AccessKeys { id: id.clone(), secret: secret.clone(), session: cfg.aws_session_token.clone().unwrap_or_default() },
        _ => return None,
    };
    Some(BedrockSettings { region, auth })
}
