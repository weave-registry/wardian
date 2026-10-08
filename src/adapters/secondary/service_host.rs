//! The service manager and other processes, for `wardian start`, `stop` and `status`
//! (ADR-2610081800): runs launchctl, systemctl, id, ps and kill, starts Wardian detached, and asks
//! an address whether a Wardian answers there.

use crate::ports::service_manager::{Answer, Ran, System};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub struct ServiceHost {
    /// Asks `addr` for Wardian's `/api/status` (the composition root passes the probe).
    pub probe: fn(&str) -> Option<Answer>,
}

impl System for ServiceHost {
    fn run(&self, program: &str, args: &[&str]) -> Option<Ran> {
        let o = Command::new(program).args(args).stdin(Stdio::null()).output().ok()?;
        let mut out = String::from_utf8_lossy(&o.stdout).into_owned();
        out.push_str(&String::from_utf8_lossy(&o.stderr));
        Some(Ran { code: o.status.code().unwrap_or(-1), out })
    }

    fn spawn(&self, program: &Path, env: &[(String, String)], dir: &Path, log: &Path) -> Result<u32, String> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(log).map_err(|e| format!("could not open {}: {e}", log.display()))?;
        let err = file.try_clone().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(program);
        cmd.envs(env.iter().map(|(k, v)| (k, v))).current_dir(dir).stdin(Stdio::null()).stdout(file).stderr(err);
        // Its own process group, so Ctrl-C in the terminal that ran `wardian start` does not reach it.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", program.display()))?;
        let pid = child.id();
        // Reaped if it ends while `wardian start` still waits, so it is not taken for running.
        std::thread::spawn(move || child.wait());
        Ok(pid)
    }

    fn wardian_at(&self, addr: &str) -> Option<Answer> {
        (self.probe)(addr)
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}
