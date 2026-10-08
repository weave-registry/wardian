//! The stop log (ADR-2610072033): every start and every stop of the server, with the time in UTC,
//! in `<data dir>/wardian.log`, and on stderr unless Wardian runs in a terminal (ADR-2610080930).
//! A panic, a signal, a failed bind and the server giving up are each written as they happen. A start that follows a start with no stop between
//! them says so, since only a kill that cannot be caught (SIGKILL, running out of memory, the
//! machine stopping) leaves no record.
//!
//! The file is kept small: past 1 MB it moves to `wardian.log.1`, replacing the one before.

use crate::ports::calendar::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;
const STARTED: &str = "started";

#[derive(Clone)]
pub struct StopLog {
    path: Arc<PathBuf>,
    /// Whether records are also written to stderr. In a terminal (ADR-2610080930) they go to the
    /// file only, and a stop by Ctrl-C says so in a few words.
    echo: bool,
}

impl StopLog {
    pub fn new(data_dir: &Path) -> StopLog {
        StopLog { path: Arc::new(data_dir.join("wardian.log")), echo: true }
    }

    /// The same log, writing records to the file only.
    pub fn quiet(self) -> StopLog {
        StopLog { echo: false, ..self }
    }

    /// Writes one line, to stderr and the file. A file that cannot be written is reported on
    /// stderr only; the log never stops Wardian.
    pub fn record(&self, what: &str) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("{} wardian {} pid {}: {what}", Utc::from_unix(now).iso(), env!("CARGO_PKG_VERSION"), std::process::id());
        if self.echo {
            eprintln!("{line}");
        }
        self.note(&line);
    }

    /// Writes a line to the file as it is: the start details Wardian prints when its output is
    /// not a terminal.
    pub fn note(&self, line: &str) {
        if let Err(e) = append(&self.path, line) {
            eprintln!("wardian: cannot write {}: {e}", self.path.display());
        }
    }

    /// Records the start, and before it the previous run's end if that left no record. Other
    /// lines (notes, and a launcher's copy of the output) may follow a record, so the last
    /// record is the one read.
    pub fn started(&self, detail: &str) {
        let last = fs::read_to_string(&*self.path).ok().and_then(|t| t.lines().rev().find(|l| is_record(l)).map(String::from));
        if let Some(last) = last.filter(|l| is_start(l)) {
            let when = last.split(' ').next().unwrap_or("?");
            let pid = last.split(" pid ").nth(1).and_then(|r| r.split(':').next()).unwrap_or("?");
            self.record(&format!(
                "the run started {when} (pid {pid}) has no stop record: it was killed with SIGKILL, ran out of memory or the machine stopped, unless it is still running"
            ));
        }
        self.record(&format!("{STARTED} ({detail})"));
    }

    /// Records panics as they happen, then lets the usual message print. A panic on the main
    /// thread stops Wardian; one in a connection's thread ends that request only.
    pub fn catch_panics(&self) {
        let log = self.clone();
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("unnamed");
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "(no message)".into());
            let at = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
            let effect = if name == "main" { "stopped by a panic" } else { "a request failed with a panic; the server keeps running" };
            log.record(&format!("{effect} in thread '{name}'{at}: {msg}"));
            default(info);
        }));
    }

    /// Records SIGINT, SIGTERM and SIGHUP (a closed terminal), then exits as the signal would.
    /// SIGHUP is caught only when stderr is a terminal: `nohup` and services redirect it and may
    /// ignore SIGHUP on purpose, and catching it would undo that.
    #[cfg(unix)]
    pub fn catch_signals(&self) {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        use std::io::IsTerminal;
        let mut wanted = vec![SIGINT, SIGTERM];
        if std::io::stderr().is_terminal() {
            wanted.push(SIGHUP);
        }
        let mut signals = match signal_hook::iterator::Signals::new(wanted) {
            Ok(s) => s,
            Err(e) => {
                self.record(&format!("signals will not be logged: {e}"));
                return;
            }
        };
        let log = self.clone();
        std::thread::spawn(move || {
            if let Some(sig) = signals.forever().next() {
                let name = match sig {
                    SIGINT => "SIGINT (Ctrl-C)",
                    SIGTERM => "SIGTERM",
                    SIGHUP => "SIGHUP (the terminal closed)",
                    _ => "a signal",
                };
                log.record(&format!("stopped by {name}"));
                if !log.echo && sig == SIGINT {
                    eprintln!("\n  Wardian stopped.");
                }
                std::process::exit(128 + sig);
            }
        });
    }

    #[cfg(not(unix))]
    pub fn catch_signals(&self) {}
}

/// Whether a line is a record: `2026-10-07T11:31:05Z wardian 0.4.0 pid 1: ...`.
fn is_record(line: &str) -> bool {
    let mut words = line.split(' ');
    let time = words.next().unwrap_or("");
    time.len() == 20 && time.ends_with('Z') && time.as_bytes()[10] == b'T' && words.next() == Some("wardian") && words.nth(1) == Some("pid")
}

fn is_start(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(_, what)| what.starts_with(STARTED))
}

fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(path).is_ok_and(|m| m.len() >= MAX_BYTES) {
        fs::rename(path, path.with_extension("log.1"))?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wardian-stoplog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn records_start_stop_and_an_unrecorded_end() {
        let dir = tmp("lines");
        let log = StopLog::new(&dir);
        log.started("listening on http://127.0.0.1:1");
        log.record("stopped by SIGTERM");
        log.started("again");
        log.started("after a kill");
        let text = fs::read_to_string(dir.join("wardian.log")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[0].ends_with("started (listening on http://127.0.0.1:1)"));
        assert!(lines[1].ends_with("stopped by SIGTERM"));
        assert!(lines[3].contains("has no stop record"), "{}", lines[3]);
        assert!(lines[4].ends_with("started (after a kill)"));
        // 2026-10-07T11:31:05Z wardian 0.4.0 pid 1: ...
        let first = lines[0].split(' ').next().unwrap();
        assert!(first.len() == 20 && first.ends_with('Z') && first.as_bytes()[10] == b'T', "{first}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn start_notes_after_a_start_do_not_hide_an_unrecorded_end() {
        let dir = tmp("notes");
        let log = StopLog::new(&dir).quiet();
        log.started("first");
        log.note("source: local dir apps");
        log.note("listening on http://x");
        log.started("second");
        let text = fs::read_to_string(dir.join("wardian.log")).unwrap();
        assert!(text.lines().nth(3).is_some_and(|l| l.contains("has no stop record")), "{text}");
        assert!(text.lines().nth(1) == Some("source: local dir apps"), "{text}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_the_file_small() {
        let dir = tmp("rotate");
        let log = StopLog::new(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("wardian.log"), vec![b'x'; usize::try_from(MAX_BYTES).unwrap()]).unwrap();
        log.record("stopped by SIGINT (Ctrl-C)");
        let now = fs::read_to_string(dir.join("wardian.log")).unwrap();
        assert!(now.lines().count() == 1 && now.contains("SIGINT"));
        assert_eq!(fs::metadata(dir.join("wardian.log.1")).unwrap().len(), MAX_BYTES);
        fs::remove_dir_all(dir).unwrap();
    }
}
