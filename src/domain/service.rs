//! Wardian as a user service (ADR-2610081800): the launchd property list and the systemd user
//! unit that run it, as text. Pure functions of what the service runs; the use case writes them.

/// What the service runs: the program, its data folder (absolute), the variables it is given and
/// where its output goes, and whether it starts at login.
pub struct Unit<'a> {
    pub label: &'a str,
    pub program: &'a str,
    pub data_dir: &'a str,
    pub log: &'a str,
    /// Variables beside DATA_DIR and WARDIAN_NO_OPEN, in order (ADDR, HOME, …).
    pub env: &'a [(String, String)],
    pub at_login: bool,
}

/// The service's variables: DATA_DIR in full, WARDIAN_NO_OPEN=1 (a service never opens a browser),
/// then the rest.
fn variables(u: &Unit<'_>) -> Vec<(String, String)> {
    let mut all = vec![("DATA_DIR".to_string(), u.data_dir.to_string()), ("WARDIAN_NO_OPEN".to_string(), "1".to_string())];
    all.extend(u.env.iter().filter(|(k, _)| k != "DATA_DIR" && k != "WARDIAN_NO_OPEN").cloned());
    all
}

/// Text for an XML element or attribute.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The launchd property list. `KeepAlive` restarts Wardian only when it exits with an error (a crash,
/// a failed start), never after `launchctl bootout`. launchd starts a job with `KeepAlive` when it
/// loads it, whatever `RunAtLoad` says, so whether it starts at login is decided by where the file
/// is kept: only `~/Library/LaunchAgents` is loaded at login.
pub fn plist(u: &Unit<'_>) -> String {
    let s = |v: &str| format!("<string>{}</string>", xml_escape(v));
    let mut env = String::new();
    for (k, v) in variables(u) {
        env.push_str(&format!("    <key>{}</key>{}\n", xml_escape(&k), s(&v)));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<!-- Written by `wardian start` (ADR-2610081800); `wardian stop` removes it. -->
<plist version=\"1.0\">
<dict>
  <key>Label</key>{label}
  <key>ProgramArguments</key><array>{program}</array>
  <key>WorkingDirectory</key>{data}
  <key>EnvironmentVariables</key>
  <dict>
{env}  </dict>
  <key>StandardOutPath</key>{log}
  <key>StandardErrorPath</key>{log}
  <key>RunAtLoad</key><{at_login}/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
</dict>
</plist>
",
        label = s(u.label),
        program = s(u.program),
        data = s(u.data_dir),
        log = s(u.log),
        at_login = u.at_login,
    )
}

/// A value for a systemd unit setting that expands specifiers: `%` written `%%`.
fn no_specifiers(s: &str) -> String {
    s.replace('%', "%%")
}

/// One word of a systemd command line or `Environment=`, in double quotes: `\` and `"` escaped,
/// `%` written `%%`, and `$` written `$$` so it is not read as a variable.
fn systemd_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The systemd user unit. `Restart=on-failure` restarts Wardian after a crash, never after
/// `systemctl --user stop`; `WantedBy=default.target` is what `systemctl --user enable` (start at
/// login) hooks it to.
pub fn systemd_unit(u: &Unit<'_>) -> String {
    let mut env = String::new();
    for (k, v) in variables(u) {
        env.push_str(&format!("Environment={}\n", systemd_quote(&format!("{k}={v}"))));
    }
    format!(
        "# Written by `wardian start` (ADR-2610081800); `wardian stop` removes it.
[Unit]
Description=Wardian ({label})

[Service]
ExecStart={program}
WorkingDirectory={data}
{env}StandardOutput=append:{log}
StandardError=append:{log}
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
",
        label = no_specifiers(u.label),
        program = systemd_quote(u.program),
        data = no_specifiers(u.data_dir),
        log = no_specifiers(u.log),
    )
}

/// The systemd unit's name for a launchd-style label: `studio.wardian` is `wardian.service`.
pub fn unit_name(label: &str) -> String {
    format!("{}.service", label.strip_prefix("studio.").unwrap_or(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit<'a>(env: &'a [(String, String)], at_login: bool) -> Unit<'a> {
        Unit {
            label: "studio.wardian",
            program: "/u/a/.local/bin/wardian",
            data_dir: "/u/a/Library/Application Support/Wardian",
            log: "/u/a/Library/Application Support/Wardian/wardian.log",
            env,
            at_login,
        }
    }

    #[test]
    fn service_plist_names_the_program_data_and_log_in_full() {
        let env = vec![("ADDR".to_string(), "127.0.0.1:8123".to_string()), ("HOME".to_string(), "/u/a".to_string())];
        let p = plist(&unit(&env, true));
        assert!(p.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist"), "{p}");
        assert!(p.contains("<key>Label</key><string>studio.wardian</string>"), "{p}");
        assert!(p.contains("<key>ProgramArguments</key><array><string>/u/a/.local/bin/wardian</string></array>"), "{p}");
        assert!(p.contains("<key>DATA_DIR</key><string>/u/a/Library/Application Support/Wardian</string>"), "{p}");
        assert!(p.contains("<key>WARDIAN_NO_OPEN</key><string>1</string>"), "{p}");
        assert!(p.contains("<key>ADDR</key><string>127.0.0.1:8123</string>"), "{p}");
        assert!(p.contains("<key>HOME</key><string>/u/a</string>"), "{p}");
        assert!(p.contains("<key>StandardOutPath</key><string>/u/a/Library/Application Support/Wardian/wardian.log</string>"), "{p}");
        assert!(p.contains("<key>StandardErrorPath</key><string>/u/a/Library/Application Support/Wardian/wardian.log</string>"), "{p}");
        assert!(p.contains("<key>WorkingDirectory</key><string>/u/a/Library/Application Support/Wardian</string>"), "{p}");
        assert!(p.contains("<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>"), "{p}");
        assert!(p.contains("<key>RunAtLoad</key><true/>"), "{p}");
        assert!(plist(&unit(&[], false)).contains("<key>RunAtLoad</key><false/>"));
        // DATA_DIR and WARDIAN_NO_OPEN come once, from the unit, whatever env says.
        let doubled = vec![("DATA_DIR".to_string(), "elsewhere".to_string()), ("WARDIAN_NO_OPEN".to_string(), "0".to_string())];
        let p = plist(&unit(&doubled, false));
        assert_eq!(p.matches("<key>DATA_DIR</key>").count(), 1);
        assert!(!p.contains("elsewhere") && p.matches("WARDIAN_NO_OPEN").count() == 1, "{p}");
    }

    #[test]
    fn service_plist_escapes_xml() {
        let u = Unit { program: "/u/a & b/<wardian>", data_dir: "/u/\"q\"/it's", ..unit(&[], false) };
        let p = plist(&u);
        assert!(p.contains("<string>/u/a &amp; b/&lt;wardian&gt;</string>"), "{p}");
        assert!(p.contains("<string>/u/&quot;q&quot;/it&apos;s</string>"), "{p}");
        assert!(!p.contains("a & b"));
        assert_eq!(xml_escape("plain"), "plain");
    }

    #[test]
    fn service_systemd_unit_quotes_and_restarts_on_failure() {
        let env = vec![("ADDR".to_string(), "127.0.0.1:8123".to_string())];
        let u = Unit { program: "/u/a/my apps/wardian", data_dir: "/u/a/.local/share/wardian 100%", log: "/u/a/.local/share/wardian 100%/wardian.log", ..unit(&env, true) };
        let t = systemd_unit(&u);
        assert!(t.contains("\n[Service]\nExecStart=\"/u/a/my apps/wardian\"\n"), "{t}");
        assert!(t.contains("\nWorkingDirectory=/u/a/.local/share/wardian 100%%\n"), "{t}");
        assert!(t.contains("\nEnvironment=\"DATA_DIR=/u/a/.local/share/wardian 100%%\"\n"), "{t}");
        assert!(t.contains("\nEnvironment=\"WARDIAN_NO_OPEN=1\"\n"), "{t}");
        assert!(t.contains("\nEnvironment=\"ADDR=127.0.0.1:8123\"\n"), "{t}");
        assert!(t.contains("\nStandardOutput=append:/u/a/.local/share/wardian 100%%/wardian.log\n"), "{t}");
        assert!(t.contains("\nStandardError=append:/u/a/.local/share/wardian 100%%/wardian.log\n"), "{t}");
        assert!(t.contains("\nRestart=on-failure\n"), "{t}");
        assert!(t.contains("\n[Install]\nWantedBy=default.target\n"), "{t}");
        assert_eq!(systemd_quote("a \"b\" \\c $HOME"), "\"a \\\"b\\\" \\\\c $$HOME\"");
    }

    #[test]
    fn service_unit_name_follows_the_label() {
        assert_eq!(unit_name("studio.wardian"), "wardian.service");
        assert_eq!(unit_name("studio.wardian.test-1f2e"), "wardian.test-1f2e.service");
        assert_eq!(unit_name("other"), "other.service");
    }
}
