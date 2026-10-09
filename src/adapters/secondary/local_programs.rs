//! The programs port on this machine (ADR-2610091530): `std::process`, no shell, a time limit.
//! Nothing a program prints is logged here.

use crate::ports::programs::{Output, Programs, RunError};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct LocalPrograms;

/// Reads a pipe to the end on its own thread, so a program that prints a lot never blocks.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

impl Programs for LocalPrograms {
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> Result<Output, RunError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunError::NotStarted(e.to_string()))?;
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunError::TimedOut);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(RunError::NotStarted(e.to_string())),
            }
        };
        Ok(Output { code: status.code(), stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn bedrock_profile_programs_run_without_a_shell_and_stop_at_the_limit() {
        let p = LocalPrograms;
        let o = p.run("/bin/echo", &["$HOME".to_string(), "a b".to_string()], Duration::from_secs(5)).unwrap();
        assert_eq!((o.code, o.stdout.as_str()), (Some(0), "$HOME a b\n"));
        assert_eq!(p.run("/bin/sleep", &["5".to_string()], Duration::from_millis(200)), Err(RunError::TimedOut));
        assert!(matches!(p.run("/no/such/program", &[], Duration::from_secs(1)), Err(RunError::NotStarted(_))));
    }
}
