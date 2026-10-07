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
    pub mod grants;
    pub mod import_plan;
    pub mod package;
    pub mod splunk;
    pub mod studio;
    pub mod suite;
}
mod ports {
    pub mod assets;
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
    pub mod docs;
    pub mod import;
    pub mod scaffold;
    pub mod splunk;
    pub mod studio;
}
mod adapters {
    pub mod primary {
        pub mod cli;
        pub mod http;
    }
    pub mod secondary {
        pub mod anthropic_inference;
        pub mod embedded_assets;
        pub mod google_drive;
        pub mod link_fetch;
        pub mod local_disk;
        pub mod splunk_rest;
    }
}

use adapters::primary::{cli, http};
use adapters::secondary::{anthropic_inference, embedded_assets::Embedded, google_drive::GoogleDrive, link_fetch::LinkFetcher, local_disk::LocalDisk, splunk_rest::SplunkRest};
use config::Settings;
use ports::{assets::Assets, service::Services, storage::FileSystem};
use std::sync::Arc;
use usecases::{
    catalog::{Hub, HubPorts},
    check::Checker,
    docs::Docs,
    scaffold::Scaffold,
    splunk::Splunk,
    studio::Studio,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fs: Arc<dyn FileSystem> = Arc::new(LocalDisk);
    let assets: Arc<dyn Assets> = Arc::new(Embedded);
    let checker = Arc::new(Checker::new(Arc::clone(&fs)));
    let tools = Scaffold::new(Arc::clone(&fs), Arc::clone(&assets), Arc::clone(&checker));
    let apps_folder = match cli::run(&args, &tools) {
        cli::Command::Exit(code) => std::process::exit(code),
        cli::Command::Serve(folder) => folder,
    };

    let cfg = Settings::from_env(&apps_folder);
    let hub = Arc::new(Hub::new(
        HubPorts {
            fs: Arc::clone(&fs),
            drive: Arc::new(GoogleDrive::new(&cfg.drive_api)),
            web: Arc::new(LinkFetcher::new(cfg.import_allow_lan)),
            assets: Arc::clone(&assets),
        },
        cfg.data_dir.clone(),
        cfg.local_root.clone(),
        cfg.refresh_every,
    ));
    let llm = Arc::new(anthropic_inference::Anthropic::new(cfg.anthropic_base.as_deref().unwrap_or(anthropic_inference::DEFAULT_BASE), cfg.ai_model.clone()));
    let studio = Studio::new(Arc::clone(&fs), llm, Arc::clone(&assets), Arc::clone(&hub), checker, &cfg.data_dir, cfg.anthropic_key.clone(), cfg.anthropic_workspace.clone());
    let splunk = Splunk::new(Arc::clone(&fs), Arc::new(SplunkRest), &cfg.data_dir, cfg.splunk.clone());
    hub.start(cfg.drive_key_file.clone(), cfg.drive_folder.clone());

    let services = Services { catalog: hub, builder: Arc::new(studio), searches: Arc::new(splunk), pages: Arc::new(Docs::new(assets)) };
    http::serve(&cfg.addr, config::ADDR_EXAMPLE, cfg.admin_token.clone(), services);
}
