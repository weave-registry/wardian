//! Wardian as a user service (ADR-2610081800). Two ports: `Background`, what `wardian start`,
//! `stop` and `status` ask for (the use case implements it), and `System`, the one way the use
//! case reaches the service manager and other processes (launchctl, systemctl, a detached start,
//! a probe of an address), which tests replace with a fake.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a program run to the end said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ran {
    /// The exit code; -1 when it was stopped by a signal.
    pub code: i32,
    /// Standard output and standard error together.
    pub out: String,
}

/// A Wardian answering `/api/status` at an address: its version and the apps folder it serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub version: Option<String>,
    pub local_root: String,
}

/// The service manager and other processes, as the use case needs them.
pub trait System: Send + Sync {
    /// Runs `program` with `args` to the end. None when it could not be started (not installed).
    fn run(&self, program: &str, args: &[&str]) -> Option<Ran>;
    /// Starts `program` with no arguments, detached from this terminal (its own process group,
    /// no input), with `env` added to this process's variables, in `dir`, its output appended to
    /// `log`. Returns its process id.
    fn spawn(&self, program: &Path, env: &[(String, String)], dir: &Path, log: &Path) -> Result<u32, String>;
    /// The Wardian answering at `addr` (host:port), if one does.
    fn wardian_at(&self, addr: &str) -> Option<Answer>;
    fn sleep(&self, d: Duration);
}

/// How Wardian runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// A launchd agent (macOS).
    Launchd,
    /// A systemd user service (Linux).
    Systemd,
    /// A process `wardian start` left running, its id in `<data>/wardian.pid`: no restart after a
    /// crash, no start at login.
    Plain,
    /// Started in a terminal with plain `wardian`.
    Terminal,
}

/// What `wardian start` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    pub url: String,
    pub how: How,
    /// True when this same Wardian was already running, so nothing was started.
    pub already: bool,
    pub at_login: bool,
    /// Another Wardian found on the usual port, named.
    pub other: Option<String>,
    pub version: String,
    pub apps: PathBuf,
    pub log: PathBuf,
    /// The service file written, if any.
    pub file: Option<PathBuf>,
}

/// What `wardian stop` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stopped {
    /// It was running this way and is stopped; the files removed.
    Stopped(How, Vec<PathBuf>),
    /// Nothing was running. A stale process id file removed, and a Wardian started in a terminal
    /// that still answers (stop does not touch it), if any.
    NotRunning { removed: Vec<PathBuf>, terminal: Option<String> },
}

/// What `wardian status` reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// How it runs; None when it does not.
    pub how: Option<How>,
    /// Its address, when it answers.
    pub url: Option<String>,
    /// The version that answers.
    pub version: Option<String>,
    pub apps: PathBuf,
    pub at_login: bool,
    pub log: PathBuf,
    /// Another Wardian found on the usual port, named.
    pub other: Option<String>,
}

impl Status {
    /// Running means answering at its address.
    pub fn running(&self) -> bool {
        self.how.is_some() && self.url.is_some()
    }
}

/// `wardian start [--at-login]`, `wardian stop`, `wardian status`.
pub trait Background {
    /// Starts Wardian as a service, unless this same Wardian already answers; waits until it does.
    fn start(&self, at_login: bool) -> Result<Started, String>;
    fn stop(&self) -> Result<Stopped, String>;
    fn status(&self) -> Status;
}
