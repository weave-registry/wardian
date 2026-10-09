//! Spike: do Wardian's domain, ports and use cases compile for the browser
//! (wasm32-unknown-unknown) with none of its adapters? This crate includes the
//! real source files from ../../src by path; it holds no copy of them.
#![allow(dead_code, unused_imports)]

mod probe;

#[path = "../../../src/domain"]
mod domain {
    pub mod agent;
    pub mod check;
    pub mod components;
    pub mod db;
    pub mod export;
    pub mod folders;
    pub mod grants;
    pub mod history;
    pub mod import_plan;
    pub mod jobs;
    pub mod package;
    pub mod service;
    pub mod splunk;
    pub mod studio;
    pub mod suite;
    pub mod usage;
    pub mod viewer_state;
}
#[path = "../../../src/ports"]
mod ports {
    pub mod assets;
    pub mod calendar;
    pub mod db;
    pub mod drive;
    pub mod llm;
    pub mod secrets;
    pub mod service;
    pub mod service_manager;
    pub mod splunk;
    pub mod storage;
    pub mod tools;
    pub mod web;
}
#[path = "../../../src/usecases"]
mod usecases {
    pub mod background;
    pub mod catalog;
    pub mod check;
    pub mod db;
    pub mod demos;
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
