//! `wardian start`, `stop` and `status` (ADR-2610081800): Wardian run by the system's service
//! manager, launchd on macOS and a systemd user service on Linux, or, without either, as a
//! detached process whose id is kept in `<data>/wardian.pid`. Every call to the service manager
//! and to other processes goes through the `System` port.

use crate::domain::service::{plist, systemd_unit, unit_name, Unit};
use crate::ports::service_manager::{Answer, Background, How, Started, Status, Stopped, System};
use crate::ports::storage::FileSystem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long `start` waits for Wardian to answer, and how often it asks.
const WAIT: Duration = Duration::from_secs(15);
const STEP: Duration = Duration::from_millis(250);
/// How long `stop` waits for a plain process to end before it is killed.
const STOP_WAIT: Duration = Duration::from_secs(5);

/// What the service runs and where its files go, decided by the composition root.
pub struct Setup {
    /// `std::env::consts::OS`.
    pub os: &'static str,
    pub home: PathBuf,
    /// XDG_CONFIG_HOME, or `~/.config`.
    pub config_home: PathBuf,
    /// The launchd label (`studio.wardian`, or WARDIAN_SERVICE_LABEL); the systemd unit is named
    /// after it.
    pub label: String,
    /// The program's full path.
    pub program: PathBuf,
    /// The data folder, in full (ADR-2610080915's rule, from where the command runs).
    pub data_dir: PathBuf,
    /// The data folder as the rule gave it, which may be relative: a Wardian started in a
    /// terminal in the same place reports its apps folder that way.
    pub data_dir_as_found: PathBuf,
    pub addr: String,
    pub addr_set: bool,
    /// Without ADDR, the server takes the first free port from ADDR's up to this one.
    pub last_port: u16,
    pub version: String,
    /// Variables written into the service beside DATA_DIR and WARDIAN_NO_OPEN.
    pub env: Vec<(String, String)>,
}

pub struct Service {
    sys: Arc<dyn System>,
    fs: Arc<dyn FileSystem>,
    s: Setup,
}

/// What was found at Wardian's addresses: this Wardian (address and answer), and a note about
/// another one.
struct Found {
    ours: Option<(String, Answer)>,
    other: Option<String>,
}

impl Service {
    pub fn new(sys: Arc<dyn System>, fs: Arc<dyn FileSystem>, s: Setup) -> Service {
        Service { sys, fs, s }
    }

    fn log(&self) -> PathBuf {
        self.s.data_dir.join("wardian.log")
    }
    fn apps(&self) -> PathBuf {
        self.s.data_dir.join("apps")
    }
    fn pid_file(&self) -> PathBuf {
        self.s.data_dir.join("wardian.pid")
    }
    /// The launchd file for a service that starts at login: only this folder is loaded at login.
    fn agent_file(&self) -> PathBuf {
        self.s.home.join("Library").join("LaunchAgents").join(format!("{}.plist", self.s.label))
    }
    /// The launchd file for a service that runs now only.
    fn now_file(&self) -> PathBuf {
        self.s.config_home.join("wardian").join(format!("{}.plist", self.s.label))
    }
    fn unit_file(&self) -> PathBuf {
        self.s.config_home.join("systemd").join("user").join(unit_name(&self.s.label))
    }

    /// The service manager here: launchd on macOS, systemd when a user session answers on Linux,
    /// else none (Plain).
    fn manager(&self) -> How {
        match self.s.os {
            "macos" => How::Launchd,
            "linux" if self.ok("systemctl", &["--user", "show-environment"]) => How::Systemd,
            _ => How::Plain,
        }
    }

    fn ok(&self, program: &str, args: &[&str]) -> bool {
        self.sys.run(program, args).is_some_and(|r| r.code == 0)
    }

    fn systemctl(&self, args: &[&str]) -> Option<crate::ports::service_manager::Ran> {
        let mut all = vec!["--user"];
        all.extend_from_slice(args);
        self.sys.run("systemctl", &all)
    }

    fn uid(&self) -> Result<String, String> {
        let r = self.sys.run("id", &["-u"]).ok_or("could not run `id -u` to find the user's launchd domain")?;
        let uid = r.out.trim().to_string();
        if r.code != 0 || uid.is_empty() || !uid.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("`id -u` did not give a user id: {}", r.out.trim()));
        }
        Ok(uid)
    }

    /// `gui/<uid>/<label>`, launchd's name for the service.
    fn target(&self) -> Result<(String, String), String> {
        let domain = format!("gui/{}", self.uid()?);
        Ok((format!("{domain}/{}", self.s.label), domain))
    }

    /// Whether the service manager has the service (loaded or active), or the plain process runs.
    fn service_runs(&self, how: How) -> bool {
        match how {
            How::Launchd => self.target().is_ok_and(|(t, _)| self.ok("launchctl", &["print", &t])),
            How::Systemd => self.systemctl(&["is-active", "--quiet", &unit_name(&self.s.label)]).is_some_and(|r| r.code == 0),
            How::Plain => self.live_pid().is_some(),
            How::Terminal => false,
        }
    }

    fn at_login(&self, how: How) -> bool {
        match how {
            How::Launchd => self.fs.exists(&self.agent_file()),
            How::Systemd => self.systemctl(&["is-enabled", "--quiet", &unit_name(&self.s.label)]).is_some_and(|r| r.code == 0),
            How::Plain | How::Terminal => false,
        }
    }

    /// The addresses Wardian may be on: ADDR's alone when it is set, else from its port up to
    /// `last_port`, as the server's port rule takes them (ADR-2610080930).
    fn addresses(&self) -> Vec<String> {
        let parsed = self.s.addr.rsplit_once(':').and_then(|(h, p)| p.parse::<u16>().ok().map(|p| (h, p)));
        match parsed.filter(|&(_, p)| !self.s.addr_set && p != 0) {
            Some((host, first)) => (first..=self.s.last_port.max(first)).map(|p| format!("{host}:{p}")).collect(),
            None => vec![self.s.addr.clone()],
        }
    }

    /// Whether a Wardian that answers is this one: this version, serving this data folder's apps.
    fn is_ours(&self, a: &Answer) -> bool {
        let roots = [self.apps(), self.s.data_dir_as_found.join("apps")];
        a.version.as_deref() == Some(self.s.version.as_str()) && roots.iter().any(|r| r.display().to_string() == a.local_root)
    }

    fn find(&self) -> Found {
        let mut other = None;
        for at in self.addresses() {
            match self.sys.wardian_at(&at) {
                Some(a) if self.is_ours(&a) => return Found { ours: Some((at, a)), other },
                Some(a) => {
                    let which = a.version.as_deref().map_or_else(|| "an older Wardian".to_string(), |v| format!("Wardian {v}"));
                    other.get_or_insert_with(|| format!("{which} serving {} answers at http://{at}; it is left alone", a.local_root));
                }
                None => {}
            }
        }
        Found { ours: None, other }
    }

    /// The process id in `wardian.pid`, when that process still runs and is a Wardian.
    fn live_pid(&self) -> Option<u32> {
        let pid: u32 = String::from_utf8_lossy(&self.fs.read(&self.pid_file())?).trim().parse().ok()?;
        self.is_wardian(pid).then_some(pid)
    }

    fn is_wardian(&self, pid: u32) -> bool {
        let r = self.sys.run("ps", &["-p", &pid.to_string(), "-o", "comm="]);
        r.is_some_and(|r| r.code == 0 && r.out.trim().ends_with("wardian"))
    }

    /// Ends a plain process: TERM, then KILL if it is still there after `STOP_WAIT`.
    fn end(&self, pid: u32) -> Result<(), String> {
        let id = pid.to_string();
        self.sys.run("kill", &["-TERM", &id]).ok_or("could not run `kill`")?;
        for _ in 0..(STOP_WAIT.as_millis() / STEP.as_millis()) {
            if !self.is_wardian(pid) {
                return Ok(());
            }
            self.sys.sleep(STEP);
        }
        self.sys.run("kill", &["-KILL", &id]);
        if self.is_wardian(pid) {
            return Err(format!("Wardian (process {pid}) did not stop"));
        }
        Ok(())
    }

    fn unit_text(&self, at_login: bool, write: fn(&Unit<'_>) -> String) -> String {
        let (program, data, log) = (self.s.program.display().to_string(), self.s.data_dir.display().to_string(), self.log().display().to_string());
        write(&Unit { label: &self.s.label, program: &program, data_dir: &data, log: &log, env: &self.s.env, at_login })
    }

    fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        self.fs.write(path, text.as_bytes()).map_err(|e| format!("could not write {}: {e}", path.display()))
    }

    /// Waits, up to `STOP_WAIT`, until launchd no longer has `target`; whether it is gone.
    fn unloaded(&self, target: &str) -> bool {
        for _ in 0..(STOP_WAIT.as_millis() / STEP.as_millis()) {
            if !self.ok("launchctl", &["print", target]) {
                return true;
            }
            self.sys.sleep(STEP);
        }
        !self.ok("launchctl", &["print", target])
    }

    /// Removes the files that exist among `paths`, and says which.
    fn remove(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| self.fs.exists(p))
            .map(|p| {
                self.fs.remove_file(p);
                p.clone()
            })
            .collect()
    }

    /// Hands the service to launchd: the file in LaunchAgents when it starts at login, else in
    /// Wardian's config folder, then `launchctl bootstrap`. A service already loaded (an older
    /// version, one that stopped answering) is booted out first.
    fn start_launchd(&self, at_login: bool) -> Result<PathBuf, String> {
        let (target, domain) = self.target()?;
        if self.ok("launchctl", &["print", &target]) {
            self.sys.run("launchctl", &["bootout", &target]);
            self.unloaded(&target);
        }
        let (file, other) = if at_login { (self.agent_file(), self.now_file()) } else { (self.now_file(), self.agent_file()) };
        self.write(&file, &self.unit_text(at_login, plist))?;
        self.remove(&[other]);
        let path = file.display().to_string();
        match self.sys.run("launchctl", &["bootstrap", &domain, &path]) {
            Some(r) if r.code == 0 => Ok(file),
            Some(r) => Err(format!("launchctl bootstrap {domain} {path} failed: {}", r.out.trim())),
            None => Err("could not run launchctl".into()),
        }
    }

    /// Hands the service to systemd: the unit file, `daemon-reload`, `restart` (which starts it
    /// when it is not running), and `enable` when it starts at login.
    fn start_systemd(&self, at_login: bool) -> Result<PathBuf, String> {
        let file = self.unit_file();
        let unit = unit_name(&self.s.label);
        self.write(&file, &self.unit_text(at_login, systemd_unit))?;
        for args in [&["daemon-reload"][..], &["restart", &unit]] {
            match self.systemctl(args) {
                Some(r) if r.code == 0 => {}
                Some(r) => return Err(format!("systemctl --user {} failed: {}", args.join(" "), r.out.trim())),
                None => return Err("could not run systemctl".into()),
            }
        }
        if at_login {
            self.systemctl(&["enable", "--quiet", &unit]).filter(|r| r.code == 0).ok_or_else(|| format!("systemctl --user enable {unit} failed"))?;
        }
        Ok(file)
    }

    /// Starts Wardian detached, its id in `wardian.pid`. One that runs but does not answer is
    /// ended first.
    fn start_plain(&self) -> Result<PathBuf, String> {
        if let Some(pid) = self.live_pid() {
            self.end(pid)?;
        }
        let env = [vec![("DATA_DIR".to_string(), self.s.data_dir.display().to_string()), ("WARDIAN_NO_OPEN".to_string(), "1".to_string())], self.s.env.clone()].concat();
        let pid = self.sys.spawn(&self.s.program, &env, &self.s.data_dir, &self.log())?;
        self.write(&self.pid_file(), &format!("{pid}\n"))?;
        Ok(self.pid_file())
    }

    fn started(&self, url: String, how: How, already: bool, at_login: bool, other: Option<String>, file: Option<PathBuf>) -> Started {
        Started { url, how, already, at_login, other, version: self.s.version.clone(), apps: self.apps(), log: self.log(), file }
    }
}

impl Background for Service {
    fn start(&self, at_login: bool) -> Result<Started, String> {
        let how = self.manager();
        let found = self.find();
        if let Some((at, _)) = found.ours {
            // This Wardian already answers: opened, not started again. As a service it can still
            // be set to start at login.
            let runs = if self.service_runs(how) { how } else { How::Terminal };
            let mut file = None;
            if at_login && runs == How::Launchd && !self.at_login(how) {
                self.write(&self.agent_file(), &self.unit_text(true, plist))?;
                self.remove(&[self.now_file()]);
                file = Some(self.agent_file());
            } else if at_login && runs == How::Systemd && !self.at_login(how) {
                self.systemctl(&["enable", "--quiet", &unit_name(&self.s.label)]);
            }
            return Ok(self.started(format!("http://{at}"), runs, true, self.at_login(runs), found.other, file));
        }
        self.fs.create_dir_all(&self.s.data_dir).map_err(|e| format!("could not make the data folder {}: {e}", self.s.data_dir.display()))?;
        // Without the flag, a service set to start at login keeps that.
        let keep = at_login || self.at_login(how);
        let file = match how {
            How::Launchd => self.start_launchd(keep)?,
            How::Systemd => self.start_systemd(keep)?,
            How::Plain | How::Terminal => self.start_plain()?,
        };
        for _ in 0..(WAIT.as_millis() / STEP.as_millis()) {
            if let Some((at, _)) = self.find().ours {
                return Ok(self.started(format!("http://{at}"), how, false, self.at_login(how), found.other, Some(file)));
            }
            if how == How::Plain && self.live_pid().is_none() {
                break;
            }
            self.sys.sleep(STEP);
        }
        Err(format!("Wardian did not answer within {} s. See its log: {}", WAIT.as_secs(), self.log().display()))
    }

    fn stop(&self) -> Result<Stopped, String> {
        let how = self.manager();
        let (was, removed) = match how {
            How::Launchd => {
                let (target, _) = self.target()?;
                let loaded = self.ok("launchctl", &["print", &target]);
                if loaded {
                    let r = self.sys.run("launchctl", &["bootout", &target]);
                    if !self.unloaded(&target) {
                        return Err(format!("launchctl bootout {target} failed: {}", r.map(|r| r.out).unwrap_or_default().trim()));
                    }
                }
                (loaded, self.remove(&[self.agent_file(), self.now_file()]))
            }
            How::Systemd => {
                let unit = unit_name(&self.s.label);
                let active = self.service_runs(How::Systemd);
                if active {
                    self.systemctl(&["stop", &unit]).filter(|r| r.code == 0).ok_or_else(|| format!("systemctl --user stop {unit} failed"))?;
                }
                if self.at_login(How::Systemd) {
                    self.systemctl(&["disable", "--quiet", &unit]);
                }
                let removed = self.remove(&[self.unit_file()]);
                if !removed.is_empty() {
                    self.systemctl(&["daemon-reload"]);
                }
                (active, removed)
            }
            How::Plain | How::Terminal => {
                let live = self.live_pid();
                if let Some(pid) = live {
                    self.end(pid)?;
                }
                (live.is_some(), self.remove(&[self.pid_file()]))
            }
        };
        if was {
            return Ok(Stopped::Stopped(how, removed));
        }
        let terminal = self.find().ours.map(|(at, _)| format!("http://{at}"));
        Ok(Stopped::NotRunning { removed, terminal })
    }

    fn status(&self) -> Status {
        let how = self.manager();
        let found = self.find();
        let runs = if self.service_runs(how) { Some(how) } else { found.ours.as_ref().map(|_| How::Terminal) };
        let (url, version) = match &found.ours {
            Some((at, a)) => (Some(format!("http://{at}")), a.version.clone()),
            None => (None, None),
        };
        Status { how: runs, url, version, apps: self.apps(), at_login: self.at_login(how), log: self.log(), other: found.other }
    }
}
