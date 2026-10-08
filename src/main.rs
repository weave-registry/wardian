//! The composition root: the one place that knows every adapter. It reads the configuration,
//! builds the adapters, hands them to the use cases as ports, and starts the command line or
//! the web server. Everything else depends inward: adapters on ports, ports and use cases on
//! the domain (see .hexa/ and `hexa analyze .`).

mod config;
#[cfg(test)]
mod tests;

mod domain {
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
    pub mod viewer_state;
}
mod ports {
    pub mod assets;
    pub mod calendar;
    pub mod db;
    pub mod drive;
    pub mod llm;
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
    pub mod scaffold;
    pub mod splunk;
    pub mod studio;
    pub mod viewer_state;
    pub mod workspace;
}
mod adapters {
    pub mod primary {
        pub mod cli;
        pub mod http;
        mod http_server;
        pub mod stop_log;
    }
    pub mod secondary {
        pub mod anthropic_inference;
        pub mod bedrock_inference;
        pub mod embedded_assets;
        pub mod google_drive;
        pub mod link_fetch;
        pub mod local_disk;
        pub mod splunk_rest;
        pub mod sqlite_store;
    }
}

use adapters::primary::{cli, http, stop_log::StopLog};
use adapters::secondary::{anthropic_inference, bedrock_inference::Bedrock, embedded_assets::Embedded, google_drive::GoogleDrive, link_fetch::LinkFetcher, local_disk::LocalDisk, splunk_rest::SplunkRest, sqlite_store::SqliteStore};
use config::Settings;
use ports::{assets::Assets, db::Database, llm::BedrockAuth, service::{Builder, Exports, Searches, Services, ViewerState}, storage::FileSystem};
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
    splunk::Splunk,
    studio::{BedrockSettings, Providers, Studio},
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
    let apps_folder = match cli::run(&args, &tools, &config::data_dir(&*fs), &exports_for_cli) {
        cli::Command::Exit(code) => std::process::exit(code),
        cli::Command::Serve(folder) => folder,
    };

    serve(Settings::from_env(apps_folder.as_deref(), &*fs));
}

/// Builds the adapters and use cases for these settings and serves them; it does not return. The
/// secrets test starts the same server in a child process.
fn serve(cfg: Settings) {
    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let assets: Arc<dyn Assets> = Arc::new(Embedded);
    let checker = Arc::new(Checker::new(Arc::clone(&fs)));
    // Before anything else: an address other machines can reach needs ADMIN_TOKEN (ADR-2610072033).
    match config::admins(&cfg.addr, cfg.admin_token.is_some()) {
        Ok(who) => println!("{who}"),
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    }
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
    println!("data: {}", data_shown.display());
    // A first start with an empty data folder shows the first-run setup once (ADR-2610072033).
    if let Err(e) = usecases::workspace::mark_first_run(&*fs, &cfg.data_dir) {
        eprintln!("data: could not prepare {}: {e}", cfg.data_dir.display());
    }
    // From here on Wardian is a server, and every way it stops is written down (ADR-2610072033).
    let stops = StopLog::new(&cfg.data_dir);
    stops.catch_panics();
    stops.catch_signals();
    stops.started(&format!("serving {} on {}", cfg.local_root.display(), cfg.addr));
    // The working folder (ADR-2610071122): filled from the example apps on the first start (found
    // by ADR-2610080915's rule), which are never changed. A folder named on the command line is
    // served as it is.
    if cfg.chosen_folder {
        if usecases::workspace::inside_git(&*fs, &cfg.local_root) {
            println!("note: {} is inside a git repository, so apps changed in Wardian show up there as uncommitted changes. Run without a folder to use {}.", cfg.local_root.display(), cfg.data_dir.join("apps").display());
        }
    } else if let Some(source) = &cfg.example_apps {
        match usecases::workspace::seed(&*fs, source, &cfg.local_root) {
            Ok(Some(n)) => println!("apps: copied {n} example app(s) from {} into {} (the working folder; {} is not changed)", source.display(), cfg.local_root.display(), source.display()),
            Ok(None) => {}
            Err(e) => eprintln!("apps: could not fill {} from {}: {e}", cfg.local_root.display(), source.display()),
        }
    } else if let Err(e) = fs.create_dir_all(&cfg.local_root) {
        eprintln!("apps: could not make {}: {e}", cfg.local_root.display());
    }
    let history = Arc::new(History::new(Arc::clone(&fs), &cfg.data_dir, &cfg.local_root));
    let hub = Arc::new(Hub::new(
        HubPorts {
            fs: Arc::clone(&fs),
            drive: Arc::new(GoogleDrive::new(&cfg.drive_api)),
            web: Arc::new(LinkFetcher::new(cfg.import_allow_lan)),
            assets: Arc::clone(&assets),
        },
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
    let studio = Studio::new(Arc::clone(&fs), providers, Arc::clone(&assets), Arc::clone(&hub), Arc::clone(&checker), &cfg.data_dir);
    let db: Arc<dyn Database> = Arc::new(SqliteStore::new(&cfg.data_dir));
    let splunk = Splunk::new(Arc::clone(&fs), Arc::new(SplunkRest), Arc::clone(&db), &cfg.data_dir, cfg.splunk.clone());
    hub.start(cfg.drive_key_file.clone(), cfg.drive_folder.clone());

    let state: Arc<dyn ViewerState> = Arc::new(State::new(Arc::clone(&fs), &cfg.data_dir));
    let exports = Arc::new(Exporter::new(Arc::clone(&fs), Arc::clone(&checker), Arc::clone(&db), Arc::clone(&state), Arc::clone(&history), &cfg.local_root, &cfg.data_dir));
    let (builder, searches): (Arc<dyn Builder>, Arc<dyn Searches>) = (Arc::new(studio), Arc::new(splunk));
    let jobs = Arc::new(JobRunner::new(Arc::clone(&searches), Arc::clone(&builder)));
    let services = Services { exports, tables: Arc::new(Db::new(db)), history, state, catalog: hub, builder, searches, jobs, pages: Arc::new(Docs::new(assets)) };
    let why = http::serve(&cfg.addr, config::ADDR_EXAMPLE, cfg.admin_token.clone(), services);
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
