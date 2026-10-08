//! What Wardian shows in a terminal (ADR-2610080930): a short start block, a note when it is
//! already running, and errors in a sentence with what to do. Everything here is a pure function
//! of its inputs (the colour choice included), except `open_browser`, which starts the system's
//! opener. When standard output is not a terminal, main.rs prints the plain lines instead.

use crate::ports::service_manager::{How, Started, Status, Stopped};
use std::path::Path;

/// The tagline under the name.
const TAGLINE: &str = "Small apps, sealed. On your machine.";

/// How to style output: not at all, the 16 basic colours, or the 256-colour palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    Off,
    Basic,
    Rich,
}

impl Colour {
    /// Colour only where it helps: on a terminal, unless `NO_COLOR` is set to anything non-empty
    /// (no-color.org) or `TERM` is `dumb`. The 256-colour palette when `TERM` or `COLORTERM`
    /// says the terminal has it.
    pub fn choose(is_tty: bool, no_color: Option<&str>, term: Option<&str>, colorterm: Option<&str>) -> Colour {
        let term = term.unwrap_or("");
        if !is_tty || no_color.is_some_and(|v| !v.is_empty()) || term == "dumb" {
            return Colour::Off;
        }
        let rich = term.contains("256color") || term.contains("kitty") || term.contains("ghostty") || matches!(colorterm, Some("truecolor" | "24bit"));
        if rich {
            Colour::Rich
        } else {
            Colour::Basic
        }
    }

    /// From this process's environment, for an output that is a terminal or not.
    pub fn from_env(is_tty: bool) -> Colour {
        let var = |k: &str| std::env::var(k).ok();
        Colour::choose(is_tty, var("NO_COLOR").as_deref(), var("TERM").as_deref(), var("COLORTERM").as_deref())
    }

    fn paint(self, basic: &str, rich: &str, s: &str) -> String {
        match self {
            Colour::Off => s.to_string(),
            Colour::Basic => format!("\x1b[{basic}m{s}\x1b[0m"),
            Colour::Rich => format!("\x1b[{rich}m{s}\x1b[0m"),
        }
    }
    fn accent(self, s: &str) -> String {
        self.paint("35", "38;5;212", s)
    }
    fn link(self, s: &str) -> String {
        self.paint("1;4;35", "1;4;38;5;212", s)
    }
    fn bold(self, s: &str) -> String {
        self.paint("1", "1", s)
    }
    fn dim(self, s: &str) -> String {
        self.paint("2", "38;5;245", s)
    }
    fn label(self, s: &str) -> String {
        self.paint("2", "38;5;246", s)
    }
    fn bad(self, s: &str) -> String {
        self.paint("1;31", "1;38;5;203", s)
    }
}

/// Whether to open the browser: only in a terminal, and not with `--no-open` or
/// `WARDIAN_NO_OPEN` set to anything but `0` or empty.
pub fn should_open(is_tty: bool, no_open_flag: bool, no_open_env: Option<&str>) -> bool {
    is_tty && !no_open_flag && !no_open_env.is_some_and(|v| !v.is_empty() && v != "0")
}

/// `path` with the home folder written `~`.
pub fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.filter(|h| h.is_absolute() && h.parent().is_some()).and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// What the start block says.
pub struct Start<'a> {
    pub version: &'a str,
    pub url: &'a str,
    /// The usual port, when it was busy and Wardian took the next free one.
    pub busy_port: Option<u16>,
    /// The apps folder, already shortened with `tilde`.
    pub apps: &'a str,
    /// How many example apps a first start copied in.
    pub added: Option<usize>,
    /// Who counts as an admin, in a few words.
    pub admin: &'a str,
    /// The log file, already shortened.
    pub log: &'a str,
    /// A note worth seeing at start, such as a served folder inside a git repository.
    pub note: Option<&'a str>,
}

const LABEL_WIDTH: usize = 10;

fn row(c: Colour, label: &str, value: &str) -> String {
    format!("    {}{value}\n", c.label(&format!("{label:<LABEL_WIDTH$}")))
}

fn header(c: Colour, title: &str, rest: &str) -> String {
    format!("\n  {} {}{}\n", c.accent("◆"), c.bold(title), rest)
}

/// The block printed when the server listens.
pub fn start_block(s: &Start<'_>, c: Colour) -> String {
    let mut out = header(c, "Wardian", &format!(" {}", c.dim(s.version)));
    out.push_str(&format!("    {}\n\n", c.dim(TAGLINE)));
    let busy = s.busy_port.map(|p| c.dim(&format!(" · {p} was in use"))).unwrap_or_default();
    out.push_str(&row(c, "Ready at", &format!("{}{busy}", c.link(s.url))));
    let added = match s.added {
        Some(1) => c.dim(" · 1 example app added"),
        Some(n) => c.dim(&format!(" · {n} example apps added")),
        None => String::new(),
    };
    out.push_str(&row(c, "Apps", &format!("{}{added}", s.apps)));
    out.push_str(&row(c, "Admin", s.admin));
    if let Some(note) = s.note {
        out.push_str(&row(c, "Note", note));
    }
    out.push_str(&format!("\n    {}\n", c.dim(&format!("Press Ctrl-C to stop · log: {}", s.log))));
    out
}

/// The block printed when a Wardian already answers on the usual address.
pub fn already_running(url: &str, opened: bool, c: Colour) -> String {
    let mut out = header(c, "Wardian is already running", "");
    let note = if opened { c.dim(" · opened in your browser") } else { String::new() };
    out.push_str(&row(c, "Open", &format!("{}{note}", c.link(url))));
    out
}

/// How Wardian runs, in a few words, with whether it starts at login (ADR-2610081800).
fn runs_as(how: How, at_login: bool) -> String {
    let login = if at_login { "starts at login" } else { "not at login" };
    match how {
        How::Launchd => format!("a launchd service · {login}"),
        How::Systemd => format!("a systemd user service · {login}"),
        How::Plain => "a background process · no restart after a crash, not at login".into(),
        How::Terminal => "a terminal (plain `wardian`)".into(),
    }
}

fn how_word(how: Option<How>) -> &'static str {
    match how {
        Some(How::Launchd) => "launchd",
        Some(How::Systemd) => "systemd",
        Some(How::Plain) => "plain",
        Some(How::Terminal) => "terminal",
        None => "none",
    }
}

fn how_to_stop(how: How) -> &'static str {
    if how == How::Terminal {
        "Stop it with Ctrl-C where it runs"
    } else {
        "Stop it with wardian stop"
    }
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// The block `wardian start` prints in a terminal.
pub fn started_block(s: &Started, opened: bool, home: Option<&Path>, c: Colour) -> String {
    let title = if s.already { "Wardian is already running" } else { "Wardian started" };
    let mut out = header(c, title, &format!(" {}", c.dim(&s.version)));
    out.push('\n');
    let note = if opened { c.dim(" · opened in your browser") } else { String::new() };
    out.push_str(&row(c, "Ready at", &format!("{}{note}", c.link(&s.url))));
    out.push_str(&row(c, "Apps", &tilde(&s.apps, home)));
    out.push_str(&row(c, "Runs as", &runs_as(s.how, s.at_login)));
    if let Some(n) = &s.other {
        out.push_str(&row(c, "Note", n));
    }
    out.push_str(&format!("\n    {}\n", c.dim(&format!("{} · log: {}", how_to_stop(s.how), tilde(&s.log, home)))));
    out
}

/// What `wardian start` prints when the output is not a terminal: one `name: value` per line.
pub fn started_lines(s: &Started) -> String {
    let what = if s.already { "already running" } else { "started" };
    let mut out = format!("wardian: {what} ({})\naddress: {}\nversion: {}\napps: {}\nat login: {}\nlog: {}\n", how_word(Some(s.how)), s.url, s.version, s.apps.display(), yes(s.at_login), s.log.display());
    if let Some(f) = &s.file {
        out.push_str(&format!("file: {}\n", f.display()));
    }
    if let Some(n) = &s.other {
        out.push_str(&format!("note: {n}\n"));
    }
    out
}

/// The block `wardian status` prints in a terminal.
pub fn status_block(s: &Status, home: Option<&Path>, c: Colour) -> String {
    let mut out = match (s.running(), &s.version) {
        (true, Some(v)) => header(c, "Wardian is running", &format!(" {}", c.dim(v))),
        (true, None) => header(c, "Wardian is running", ""),
        (false, _) => header(c, "Wardian is not running", ""),
    };
    out.push('\n');
    if let Some(url) = &s.url {
        out.push_str(&row(c, "Ready at", &c.link(url)));
    }
    out.push_str(&row(c, "Apps", &tilde(&s.apps, home)));
    match s.how {
        Some(how) => out.push_str(&row(c, "Runs as", &runs_as(how, s.at_login))),
        None if s.at_login => out.push_str(&row(c, "At login", "starts at login")),
        None => {}
    }
    if s.how.is_some() && s.url.is_none() {
        out.push_str(&row(c, "Note", "the service is there but does not answer; see the log"));
    }
    if let Some(n) = &s.other {
        out.push_str(&row(c, "Note", n));
    }
    let next = match s.how {
        Some(how) if s.running() => how_to_stop(how).to_string(),
        Some(_) => "Start it again with wardian start".into(),
        None => "Start it with wardian start (--at-login: also when you log in)".into(),
    };
    out.push_str(&format!("\n    {}\n", c.dim(&format!("{next} · log: {}", tilde(&s.log, home)))));
    out
}

/// What `wardian status` prints when the output is not a terminal.
pub fn status_lines(s: &Status) -> String {
    let mut out = format!("running: {}\nhow: {}\n", yes(s.running()), how_word(s.how));
    if let Some(url) = &s.url {
        out.push_str(&format!("address: {url}\n"));
    }
    if let Some(v) = &s.version {
        out.push_str(&format!("version: {v}\n"));
    }
    out.push_str(&format!("apps: {}\nat login: {}\nlog: {}\n", s.apps.display(), yes(s.at_login), s.log.display()));
    if let Some(n) = &s.other {
        out.push_str(&format!("note: {n}\n"));
    }
    out
}

/// What `wardian stop` prints: styled in a terminal (`c` not Off), plain lines otherwise.
pub fn stopped_text(s: &Stopped, home: Option<&Path>, c: Colour, tty: bool) -> String {
    let removed = |list: &[std::path::PathBuf]| list.iter().map(|p| tilde(p, home)).collect::<Vec<_>>().join(", ");
    match (s, tty) {
        (Stopped::Stopped(how, files), true) => {
            let mut out = header(c, "Wardian stopped", "");
            out.push('\n');
            out.push_str(&row(c, "Was", &runs_as(*how, false).replace(" · not at login", "")));
            if !files.is_empty() {
                out.push_str(&row(c, "Removed", &removed(files)));
            }
            out
        }
        (Stopped::NotRunning { removed: files, terminal }, true) => {
            let mut out = header(c, "Wardian is not running", "");
            if !files.is_empty() || terminal.is_some() {
                out.push('\n');
            }
            if !files.is_empty() {
                out.push_str(&row(c, "Removed", &removed(files)));
            }
            if let Some(url) = terminal {
                out.push_str(&row(c, "Note", &format!("a Wardian started in a terminal answers at {url}; stop it with Ctrl-C there")));
            }
            out
        }
        (Stopped::Stopped(how, files), false) => {
            let mut out = format!("wardian: stopped ({})\n", how_word(Some(*how)));
            for f in files {
                out.push_str(&format!("removed: {}\n", f.display()));
            }
            out
        }
        (Stopped::NotRunning { removed: files, terminal }, false) => {
            let mut out = "wardian: not running\n".to_string();
            for f in files {
                out.push_str(&format!("removed: {}\n", f.display()));
            }
            if let Some(url) = terminal {
                out.push_str(&format!("note: a Wardian started in a terminal answers at {url}; stop it with Ctrl-C there\n"));
            }
            out
        }
    }
}

/// An error: what went wrong, then what to do, for standard error.
pub fn error_block(what: &str, todo: &str, c: Colour) -> String {
    format!("\n  {} {what}\n    {todo}\n", c.bad("✗"))
}

/// Opens `url` in the default browser with `open` (macOS) or `xdg-open` (elsewhere), without
/// waiting for it and without letting it write to the terminal. Returns whether it started.
pub fn open_browser(url: &str) -> bool {
    use std::process::{Command, Stdio};
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    match Command::new(opener).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(busy: Option<u16>, added: Option<usize>) -> Start<'static> {
        Start {
            version: "0.4.0",
            url: "http://[::1]:8000",
            busy_port: busy,
            apps: "~/Wardian/apps",
            added,
            admin: "this computer only",
            log: "~/Wardian/wardian.log",
            note: None,
        }
    }

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn start_block_without_colour_has_no_escape_codes() {
        let block = start_block(&sample(None, Some(5)), Colour::Off);
        assert!(!block.contains('\x1b'), "{block}");
        let expected = "
  ◆ Wardian 0.4.0
    Small apps, sealed. On your machine.

    Ready at  http://[::1]:8000
    Apps      ~/Wardian/apps · 5 example apps added
    Admin     this computer only

    Press Ctrl-C to stop · log: ~/Wardian/wardian.log
";
        assert_eq!(block, expected);
        assert!(block.lines().count() <= 10);
    }

    #[test]
    fn start_block_with_colour_styles_and_reads_the_same() {
        for c in [Colour::Basic, Colour::Rich] {
            let block = start_block(&sample(None, Some(5)), c);
            assert!(block.contains("\x1b["), "{block}");
            assert!(block.contains("\x1b[0m"));
            // The address is bold, underlined and in the accent colour.
            assert!(block.contains(if c == Colour::Rich { "\x1b[1;4;38;5;212mhttp://[::1]:8000\x1b[0m" } else { "\x1b[1;4;35mhttp://[::1]:8000\x1b[0m" }), "{block}");
            assert_eq!(strip(&block), start_block(&sample(None, Some(5)), Colour::Off));
        }
    }

    #[test]
    fn start_block_says_a_busy_port_and_leaves_out_what_did_not_happen() {
        let block = start_block(&sample(Some(8000), None), Colour::Off);
        assert!(block.contains("Ready at  http://[::1]:8000 · 8000 was in use\n"), "{block}");
        assert!(block.contains("Apps      ~/Wardian/apps\n"), "{block}");
        assert!(start_block(&sample(None, Some(1)), Colour::Off).contains("· 1 example app added"));
        let noted = start_block(&Start { note: Some("~/src/apps is inside a git repository"), ..sample(None, None) }, Colour::Off);
        assert!(noted.contains("    Admin     this computer only\n    Note      ~/src/apps is inside a git repository\n"), "{noted}");
    }

    #[test]
    fn start_already_running_and_errors() {
        let b = already_running("http://[::1]:8000", true, Colour::Off);
        assert_eq!(b, "\n  ◆ Wardian is already running\n    Open      http://[::1]:8000 · opened in your browser\n");
        assert!(!already_running("http://x", false, Colour::Off).contains("opened"));
        let e = error_block("Wardian cannot listen on [::1]:9: in use.", "Stop it.", Colour::Off);
        assert_eq!(e, "\n  ✗ Wardian cannot listen on [::1]:9: in use.\n    Stop it.\n");
        assert!(error_block("a", "b", Colour::Basic).contains("\x1b[1;31m✗\x1b[0m"));
    }

    fn started(already: bool, how: How) -> Started {
        Started {
            url: "http://127.0.0.1:8000".into(),
            how,
            already,
            at_login: true,
            other: None,
            version: "0.4.5".into(),
            apps: PathBuf::from("/u/a/Library/Application Support/Wardian/apps"),
            log: PathBuf::from("/u/a/Library/Application Support/Wardian/wardian.log"),
            file: Some(PathBuf::from("/u/a/Library/LaunchAgents/studio.wardian.plist")),
        }
    }

    #[test]
    fn service_start_and_status_blocks_read_like_the_start_block() {
        let home = PathBuf::from("/u/a");
        let b = started_block(&started(false, How::Launchd), true, Some(&home), Colour::Off);
        assert_eq!(
            b,
            "
  ◆ Wardian started 0.4.5

    Ready at  http://127.0.0.1:8000 · opened in your browser
    Apps      ~/Library/Application Support/Wardian/apps
    Runs as   a launchd service · starts at login

    Stop it with wardian stop · log: ~/Library/Application Support/Wardian/wardian.log
"
        );
        assert_eq!(strip(&started_block(&started(false, How::Launchd), true, Some(&home), Colour::Rich)), b);
        assert!(started_block(&started(true, How::Terminal), false, Some(&home), Colour::Off).contains("◆ Wardian is already running 0.4.5\n"));
        assert!(started_block(&started(true, How::Terminal), false, Some(&home), Colour::Off).contains("Stop it with Ctrl-C where it runs"));
        let lines = started_lines(&started(false, How::Launchd));
        assert!(lines.starts_with("wardian: started (launchd)\naddress: http://127.0.0.1:8000\nversion: 0.4.5\n"), "{lines}");
        assert!(lines.contains("at login: yes\n") && lines.contains("file: /u/a/Library/LaunchAgents/studio.wardian.plist\n"), "{lines}");

        let running = Status { how: Some(How::Systemd), url: Some("http://127.0.0.1:8000".into()), version: Some("0.4.5".into()), apps: "/u/a/apps".into(), at_login: false, log: "/u/a/wardian.log".into(), other: None };
        assert_eq!(
            status_block(&running, Some(&home), Colour::Off),
            "\n  ◆ Wardian is running 0.4.5\n\n    Ready at  http://127.0.0.1:8000\n    Apps      ~/apps\n    Runs as   a systemd user service · not at login\n\n    Stop it with wardian stop · log: ~/wardian.log\n"
        );
        assert_eq!(status_lines(&running), "running: yes\nhow: systemd\naddress: http://127.0.0.1:8000\nversion: 0.4.5\napps: /u/a/apps\nat login: no\nlog: /u/a/wardian.log\n");
        let stopped = Status { how: None, url: None, version: None, at_login: true, ..running };
        let b = status_block(&stopped, Some(&home), Colour::Off);
        assert!(b.starts_with("\n  ◆ Wardian is not running\n") && b.contains("At login  starts at login\n") && b.contains("Start it with wardian start"), "{b}");
        assert!(status_lines(&stopped).starts_with("running: no\nhow: none\napps:"));

        let s = stopped_text(&Stopped::Stopped(How::Launchd, vec![PathBuf::from("/u/a/Library/LaunchAgents/studio.wardian.plist")]), Some(&home), Colour::Off, true);
        assert_eq!(s, "\n  ◆ Wardian stopped\n\n    Was       a launchd service\n    Removed   ~/Library/LaunchAgents/studio.wardian.plist\n");
        assert_eq!(stopped_text(&Stopped::NotRunning { removed: vec![], terminal: None }, Some(&home), Colour::Off, false), "wardian: not running\n");
    }

    #[test]
    fn start_colour_follows_no_color_term_and_the_terminal() {
        assert_eq!(Colour::choose(true, None, Some("xterm-256color"), None), Colour::Rich);
        assert_eq!(Colour::choose(true, None, Some("xterm"), None), Colour::Basic);
        assert_eq!(Colour::choose(true, None, Some("xterm"), Some("truecolor")), Colour::Rich);
        assert_eq!(Colour::choose(true, None, None, None), Colour::Basic);
        // NO_COLOR set to anything non-empty turns it off; empty does not (no-color.org).
        assert_eq!(Colour::choose(true, Some("1"), Some("xterm-256color"), None), Colour::Off);
        assert_eq!(Colour::choose(true, Some(""), Some("xterm-256color"), None), Colour::Rich);
        assert_eq!(Colour::choose(true, None, Some("dumb"), Some("truecolor")), Colour::Off);
        assert_eq!(Colour::choose(false, None, Some("xterm-256color"), Some("truecolor")), Colour::Off);
    }

    #[test]
    fn start_opens_the_browser_only_in_a_terminal_and_when_not_told_otherwise() {
        assert!(should_open(true, false, None));
        assert!(should_open(true, false, Some("")));
        assert!(should_open(true, false, Some("0")));
        assert!(!should_open(true, false, Some("1")));
        assert!(!should_open(true, true, None));
        assert!(!should_open(false, false, None));
    }

    #[test]
    fn start_paths_under_home_use_tilde() {
        let home = PathBuf::from("/u/a");
        assert_eq!(tilde(Path::new("/u/a/Library/Application Support/Wardian/apps"), Some(&home)), "~/Library/Application Support/Wardian/apps");
        assert_eq!(tilde(Path::new("/u/a"), Some(&home)), "~");
        assert_eq!(tilde(Path::new("/u/ab/x"), Some(&home)), "/u/ab/x");
        assert_eq!(tilde(Path::new("/srv/apps"), Some(&home)), "/srv/apps");
        assert_eq!(tilde(Path::new("data/apps"), Some(&home)), "data/apps");
        assert_eq!(tilde(Path::new("/x/y"), None), "/x/y");
        // A home of "/" or a relative one is not shortened.
        assert_eq!(tilde(Path::new("/x/y"), Some(Path::new("/"))), "/x/y");
        assert_eq!(tilde(Path::new("rel/x"), Some(Path::new("rel"))), "rel/x");
    }
}
