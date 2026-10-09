//! Running another program and reading what it prints (ADR-2610091530): the AWS CLI, or an AWS
//! profile's `credential_process`. Tests replace it with a fake.

use std::time::Duration;

/// What a program that ran to the end printed. Its standard output can hold keys: the caller
/// reads it and never logs or repeats it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The exit code; None when a signal stopped it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// It could not be started: the operating system's words.
    NotStarted(String),
    /// It ran longer than allowed, and was stopped.
    TimedOut,
}

pub trait Programs: Send + Sync {
    /// Runs `program` with `args`, directly (no shell), with no input and this process's
    /// variables, and stops it after `timeout`.
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> Result<Output, RunError>;
}
