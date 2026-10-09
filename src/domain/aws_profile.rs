//! AWS profiles for Amazon Bedrock (ADR-2610091530): the INI form of `~/.aws/config` and
//! `~/.aws/credentials`, which profile signs in how, the JSON a `credential_process` (or
//! `aws configure export-credentials --format process`) prints, and the words for each failure.
//! Pure functions: the use case reads the files and runs the programs.
//!
//! Wardian keeps a profile's name and region, never its keys. The keys these functions return
//! live in memory only, and no message here ever carries one.

/// Credentials are fetched again this long before they expire.
const REFRESH_EARLY_SECS: u64 = 300;
/// The longest the AWS CLI or a `credential_process` may run.
pub const RUN_TIMEOUT_SECS: u64 = 30;
/// Where the AWS CLI is looked for after the folders on PATH: a service started at login has a
/// bare PATH, and these are where the installers and Homebrew put `aws`.
const CLI_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

/// One `[section]` of an INI file, its keys in order (a later key replaces an earlier one).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Section {
    pub name: String,
    pub keys: Vec<(String, String)>,
}

/// The INI form the AWS tools read: `[section]` headers, `key = value` lines, whole-line comments
/// starting with `#` or `;`, CRLF or LF. A value keeps its inner spaces. Indented lines under a key
/// (nested settings such as `s3 =`) are skipped. Keys are lower-cased; a section seen twice is one
/// section.
fn parse_ini(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut current: Option<usize> = None;
    for raw in text.trim_start_matches('\u{feff}').split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = inner.split_whitespace().collect::<Vec<_>>().join(" ");
            current = Some(match out.iter().position(|s| s.name == name) {
                Some(i) => i,
                None => {
                    out.push(Section { name, keys: Vec::new() });
                    out.len() - 1
                }
            });
            continue;
        }
        // A nested setting belongs to the key above it; Wardian needs none of them.
        if raw.starts_with([' ', '\t']) {
            continue;
        }
        let (Some(i), Some((k, v))) = (current, line.split_once('=')) else { continue };
        let key = k.trim().to_ascii_lowercase();
        let keys = &mut out[i].keys;
        keys.retain(|(old, _)| *old != key);
        keys.push((key, v.trim().to_string()));
    }
    out
}

/// The profile a config-file section names: `[default]`, `[profile x]`. Other sections
/// (`[sso-session x]`, `[services x]`, a bare `[x]`) are not profiles there.
fn config_profile(section: &str) -> Option<&str> {
    match section.strip_prefix("profile ") {
        Some(name) => Some(name.trim()),
        None if section == "default" => Some("default"),
        None => None,
    }
}

/// The two files, as read: each one's path (for messages) and its text, None when not there.
pub struct AwsFiles<'a> {
    pub config_path: &'a str,
    pub config: Option<&'a str>,
    pub credentials_path: &'a str,
    pub credentials: Option<&'a str>,
}

/// A profile as Settings lists it: never its keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileInfo {
    pub name: String,
    pub region: Option<String>,
}

/// The profiles both files name, `default` first, then by name, each with its region.
pub fn profile_names(files: &AwsFiles) -> Vec<ProfileInfo> {
    let mut names: Vec<String> = Vec::new();
    for s in parse_ini(files.config.unwrap_or("")) {
        if let Some(n) = config_profile(&s.name) {
            names.push(n.to_string());
        }
    }
    for s in parse_ini(files.credentials.unwrap_or("")) {
        names.push(s.name);
    }
    names.sort_by(|a, b| (a != "default", a).cmp(&(b != "default", b)));
    names.dedup();
    names
        .into_iter()
        .filter(|n| !n.is_empty())
        .map(|name| {
            let region = merged(files, &name).and_then(|m| get(&m, "region"));
            ProfileInfo { name, region }
        })
        .collect()
}

/// The keys a profile has: the config file's, then the credentials file's on top (the AWS CLI
/// lets the credentials file win). Each key says which file it came from. None when neither file
/// has the profile.
fn merged(files: &AwsFiles, name: &str) -> Option<Vec<(String, String, From)>> {
    let mut keys: Vec<(String, String, From)> = Vec::new();
    let mut found = false;
    for s in parse_ini(files.config.unwrap_or("")) {
        if config_profile(&s.name) == Some(name) {
            found = true;
            for (k, v) in s.keys {
                keys.retain(|(old, _, _)| *old != k);
                keys.push((k, v, From::Config));
            }
        }
    }
    for s in parse_ini(files.credentials.unwrap_or("")) {
        if s.name == name {
            found = true;
            for (k, v) in s.keys {
                keys.retain(|(old, _, _)| *old != k);
                keys.push((k, v, From::Credentials));
            }
        }
    }
    found.then_some(keys)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum From {
    Config,
    Credentials,
}

fn get(keys: &[(String, String, From)], key: &str) -> Option<String> {
    keys.iter().find(|(k, _, _)| k == key).map(|(_, v, _)| v.clone()).filter(|v| !v.is_empty())
}

/// Keys from AWS: the access key ID, the secret, a session token (empty without one), and when
/// they stop working (seconds since 1970), None for keys that do not expire.
#[derive(Clone, PartialEq, Eq)]
pub struct Creds {
    pub id: String,
    pub secret: String,
    pub session: String,
    pub expires: Option<u64>,
}

impl Creds {
    /// True while the keys may still be used: until five minutes before they expire.
    pub fn fresh(&self, now: u64) -> bool {
        self.expires.is_none_or(|e| now.saturating_add(REFRESH_EARLY_SECS) < e)
    }
}

/// Never prints a key, so a stray `{:?}` cannot leak one.
impl std::fmt::Debug for Creds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Creds {{ expires: {:?} }}", self.expires)
    }
}

/// What kind of sign-in a profile needs, the way the AWS CLI decides: an assumed role first, then
/// IAM Identity Center (SSO), then keys in the credentials file, then `credential_process`, then
/// keys in the config file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Keys written in one of the files.
    Keys(Creds),
    /// A program that prints the process JSON.
    Process(String),
    /// Only the AWS CLI can sign in: an SSO profile or an assumed role (the words say which).
    NeedsCli(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub region: Option<String>,
    pub source: Source,
}

/// Looks a profile up in both files.
pub fn resolve(files: &AwsFiles, name: &str) -> Result<Profile, String> {
    let keys = merged(files, name).ok_or_else(|| unknown_profile(name, files))?;
    let region = get(&keys, "region");
    let static_keys = |from: From| -> Option<Creds> {
        let from_here = |k: &str| keys.iter().find(|(key, _, f)| key == k && *f == from).map(|(_, v, _)| v.clone()).filter(|v| !v.is_empty());
        Some(Creds { id: from_here("aws_access_key_id")?, secret: from_here("aws_secret_access_key")?, session: from_here("aws_session_token").unwrap_or_default(), expires: None })
    };
    let source = if get(&keys, "role_arn").is_some() {
        Source::NeedsCli("assumes a role (role_arn)")
    } else if get(&keys, "sso_session").is_some() || get(&keys, "sso_start_url").is_some() {
        Source::NeedsCli("signs in through IAM Identity Center (SSO)")
    } else if let Some(c) = static_keys(From::Credentials) {
        Source::Keys(c)
    } else if let Some(cmd) = get(&keys, "credential_process") {
        Source::Process(cmd)
    } else if let Some(c) = static_keys(From::Config) {
        Source::Keys(c)
    } else {
        return Err(format!("AWS profile \"{name}\" has no keys, credential_process, SSO or role to sign in with"));
    };
    Ok(Profile { name: name.to_string(), region, source })
}

/// "No such profile", naming the files read.
fn unknown_profile(name: &str, files: &AwsFiles) -> String {
    let file = |path: &str, text: Option<&str>| if text.is_some() { path.to_string() } else { format!("{path} (not there)") };
    format!(
        "there is no AWS profile \"{name}\" in {} or {}",
        file(files.config_path, files.config),
        file(files.credentials_path, files.credentials)
    )
}

/// An SSO or role profile while no AWS CLI v2 is installed.
pub fn needs_cli(name: &str, why: &str) -> String {
    format!("AWS profile \"{name}\" {why}, which needs AWS CLI v2: install it, then save the profile again in Settings → Claude")
}

/// A profile, and Settings, without a region.
pub fn no_region(name: &str) -> String {
    format!("AWS profile \"{name}\" has no region: choose one, such as us-east-1")
}

/// What to do when the SSO sign-in has run out.
pub fn sso_expired(name: &str) -> String {
    format!("the AWS sign-in of profile \"{name}\" has expired. Run `aws sso login --profile {name}`, then try again")
}

/// True when the AWS CLI's error says the SSO token or session has expired, or was never there
/// ("Token has expired and refresh failed", "The SSO session associated with this profile has
/// expired or is otherwise invalid", "Error loading SSO Token: Token for … does not exist").
pub fn says_sso_expired(output: &str) -> bool {
    let o = output.to_ascii_lowercase();
    let about_sso = o.contains("sso") || o.contains("token");
    about_sso && (o.contains("expired") || (o.contains("sso token") && o.contains("does not exist")) || o.contains("sso session") && o.contains("invalid"))
}

/// True when the AWS CLI is version 1, which has no `configure export-credentials`.
pub fn says_cli_v1(output: &str) -> bool {
    let o = output.to_ascii_lowercase();
    o.contains("invalid choice") || o.contains("aws-cli/1.")
}

/// The last line a failed program wrote to standard error, cut short: what it said went wrong.
/// Standard output is never shown, since it is where keys are printed.
pub fn last_words(stderr: &str) -> String {
    let line = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("");
    line.chars().take(300).collect()
}

/// The arguments that ask the AWS CLI for a profile's keys.
pub fn export_args(name: &str) -> Vec<String> {
    ["configure", "export-credentials", "--profile", name, "--format", "process"].iter().map(|s| s.to_string()).collect()
}

/// Reads the process JSON: `{"Version": 1, "AccessKeyId", "SecretAccessKey", "SessionToken",
/// "Expiration"}`. `what` names the program for the message; the output itself is never quoted.
pub fn parse_process_json(stdout: &str, what: &str) -> Result<Creds, String> {
    let bad = |why: &str| format!("{what} did not print the credentials JSON ({why})");
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|_| bad("not JSON"))?;
    if v["Version"].as_u64() != Some(1) {
        return Err(bad("\"Version\" must be 1"));
    }
    let s = |k: &str| v[k].as_str().map(str::trim).filter(|x| !x.is_empty()).map(String::from);
    let (Some(id), Some(secret)) = (s("AccessKeyId"), s("SecretAccessKey")) else {
        return Err(bad("AccessKeyId and SecretAccessKey are needed"));
    };
    let expires = match s("Expiration") {
        Some(e) => Some(parse_time(&e).ok_or_else(|| bad("Expiration is not an ISO 8601 time"))?),
        None => None,
    };
    Ok(Creds { id, secret, session: s("SessionToken").unwrap_or_default(), expires })
}

/// An ISO 8601 time as the AWS tools print it (`2026-10-09T15:30:00Z`, with or without fractions
/// of a second, `Z` or an offset such as `+00:00`), in seconds since 1970.
fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let (clock, offset) = if let Some(c) = rest.strip_suffix(['Z', 'z']) {
        (c, 0)
    } else if let Some(i) = rest.rfind(['+', '-']) {
        let (c, off) = rest.split_at(i);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':').unwrap_or((&off[1..off.len().min(3)], off.get(3..).unwrap_or("0")));
        (c, sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60))
    } else {
        (rest, 0)
    };
    let mut t = clock.split(':');
    let (h, min) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let sec = t.next().map(|x| x.split('.').next().unwrap_or("0").parse::<i64>().ok()).unwrap_or(Some(0))?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || min > 59 || sec > 60 {
        return None;
    }
    let total = days_from_civil(y, m, day) * 86_400 + h * 3600 + min * 60 + sec - offset;
    u64::try_from(total).ok()
}

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar (Howard Hinnant's method).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Splits a `credential_process` command line into the program and its arguments, as the AWS CLI
/// does on Linux and macOS (POSIX shell words), without running a shell: words split at spaces;
/// '…' keeps everything literally; "…" keeps spaces and lets `\"` and `\\` through; a backslash
/// outside quotes keeps the next character. No variables, globs, pipes or `~`.
pub fn split_command(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(x) => word.push(x),
                        None => return Err("credential_process has a ' that is not closed".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(x @ ('"' | '\\' | '$' | '`')) => word.push(x),
                            Some(x) => {
                                word.push('\\');
                                word.push(x);
                            }
                            None => return Err("credential_process has a \" that is not closed".into()),
                        },
                        Some(x) => word.push(x),
                        None => return Err("credential_process has a \" that is not closed".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(x) = chars.next() {
                    word.push(x);
                }
            }
            x => {
                in_word = true;
                word.push(x);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    if words.is_empty() {
        return Err("credential_process is empty".into());
    }
    Ok(words)
}

/// Where to look for `aws`, in order: each folder on PATH, then [`CLI_DIRS`].
pub fn cli_candidates(path_var: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let dirs = path_var.unwrap_or("").split(':').filter(|d| !d.is_empty()).map(String::from).chain(CLI_DIRS.iter().map(|d| d.to_string()));
    for dir in dirs {
        let candidate = format!("{}/aws", dir.trim_end_matches('/'));
        if !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

/// A profile name Wardian accepts from Settings: what the AWS tools allow in a section header.
pub fn valid_profile_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.@+/:=,".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "# my AWS settings\r\n[default]\r\nregion = us-east-1\r\n\r\n; work account\r\n[profile work]\r\nregion=eu-west-1\r\nsso_session = corp\r\nsso_account_id = 111122223333\r\nsso_role_name = Dev\r\n\r\n[sso-session corp]\r\nsso_start_url = https://corp.awsapps.com/start\r\nsso_region = us-east-1\r\n\r\n[profile  proc ]\r\nregion = us-west-2\r\ncredential_process = \"/opt/my tools/creds\" --account 'dev team' plain\r\ns3 =\r\n  max_concurrent_requests = 20\r\n[profile role]\r\nrole_arn = arn:aws:iam::123:role/x\r\nsource_profile = default\r\n[profile both]\r\naws_access_key_id = CONFIGID\r\naws_secret_access_key = config secret\r\nregion = ap-south-1\r\n[profile cfgkeys]\r\naws_access_key_id = CFGID\r\naws_secret_access_key = cfgsecret\r\n[notaprofile]\r\nregion = xx\r\n";
    const CREDENTIALS: &str = "[default]\naws_access_key_id = DEFAULTID\naws_secret_access_key = default/secret+key\n# a comment\n[both]\naws_access_key_id = CREDID\naws_secret_access_key = cred secret\naws_session_token = tok\n[onlycreds]\naws_access_key_id = ONLYID\naws_secret_access_key = onlysecret\nregion = sa-east-1\n";

    fn files<'a>(config: Option<&'a str>, credentials: Option<&'a str>) -> AwsFiles<'a> {
        AwsFiles { config_path: "/h/.aws/config", config, credentials_path: "/h/.aws/credentials", credentials }
    }

    #[test]
    fn bedrock_profile_ini_comments_crlf_spaces_and_sections() {
        let s = parse_ini(CONFIG);
        let names: Vec<&str> = s.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["default", "profile work", "sso-session corp", "profile proc", "profile role", "profile both", "profile cfgkeys", "notaprofile"]);
        let proc = &s[3].keys;
        assert_eq!(proc[1], ("credential_process".to_string(), "\"/opt/my tools/creds\" --account 'dev team' plain".to_string()));
        assert!(!proc.iter().any(|(k, _)| k == "max_concurrent_requests"), "nested settings are skipped: {proc:?}");
        assert_eq!(s[1].keys[0], ("region".to_string(), "eu-west-1".to_string()));
        assert!(parse_ini("# only\n; comments\n").is_empty());
        assert_eq!(parse_ini("[a]\nk = 1\n[a]\nk = 2\n")[0].keys, [("k".to_string(), "2".to_string())]);
    }

    #[test]
    fn bedrock_profile_names_from_both_files_with_regions() {
        let got = profile_names(&files(Some(CONFIG), Some(CREDENTIALS)));
        let pairs: Vec<(&str, Option<&str>)> = got.iter().map(|p| (p.name.as_str(), p.region.as_deref())).collect();
        assert_eq!(
            pairs,
            [("default", Some("us-east-1")), ("both", Some("ap-south-1")), ("cfgkeys", None), ("onlycreds", Some("sa-east-1")), ("proc", Some("us-west-2")), ("role", None), ("work", Some("eu-west-1"))]
        );
        assert!(profile_names(&files(None, None)).is_empty());
    }

    #[test]
    fn bedrock_profile_resolution_order() {
        let f = files(Some(CONFIG), Some(CREDENTIALS));
        // The credentials file's keys win over the config file's.
        let both = resolve(&f, "both").unwrap();
        assert_eq!(both.source, Source::Keys(Creds { id: "CREDID".into(), secret: "cred secret".into(), session: "tok".into(), expires: None }));
        assert_eq!(both.region.as_deref(), Some("ap-south-1"));
        assert_eq!(resolve(&f, "default").unwrap().source, Source::Keys(Creds { id: "DEFAULTID".into(), secret: "default/secret+key".into(), session: String::new(), expires: None }));
        assert!(matches!(resolve(&f, "cfgkeys").unwrap().source, Source::Keys(Creds { ref id, .. }) if id == "CFGID"));
        assert_eq!(resolve(&f, "proc").unwrap().source, Source::Process("\"/opt/my tools/creds\" --account 'dev team' plain".into()));
        assert_eq!(resolve(&f, "work").unwrap().source, Source::NeedsCli("signs in through IAM Identity Center (SSO)"));
        assert_eq!(resolve(&f, "role").unwrap().source, Source::NeedsCli("assumes a role (role_arn)"));
        // A bare [x] in the config file is not a profile.
        assert!(resolve(&f, "notaprofile").is_err());
        // Keys in the credentials file win over credential_process in the config file.
        let mixed = files(Some("[profile m]\ncredential_process = /bin/creds\n"), Some("[m]\naws_access_key_id = A\naws_secret_access_key = B\n"));
        assert!(matches!(resolve(&mixed, "m").unwrap().source, Source::Keys(_)));
        // …but credential_process wins over keys in the config file.
        let mixed = files(Some("[profile m]\ncredential_process = /bin/creds\naws_access_key_id = A\naws_secret_access_key = B\n"), None);
        assert_eq!(resolve(&mixed, "m").unwrap().source, Source::Process("/bin/creds".into()));
        assert!(resolve(&files(Some("[profile e]\nregion = us-east-1\n"), None), "e").unwrap_err().contains("has no keys"));
    }

    #[test]
    fn bedrock_profile_messages() {
        let f = files(Some(CONFIG), None);
        assert_eq!(resolve(&f, "nope").unwrap_err(), "there is no AWS profile \"nope\" in /h/.aws/config or /h/.aws/credentials (not there)");
        assert_eq!(no_region("work"), "AWS profile \"work\" has no region: choose one, such as us-east-1");
        assert_eq!(sso_expired("work"), "the AWS sign-in of profile \"work\" has expired. Run `aws sso login --profile work`, then try again");
        assert!(needs_cli("work", "signs in through IAM Identity Center (SSO)").contains("needs AWS CLI v2"));
        for said in [
            "Error when retrieving token from sso: Token has expired and refresh failed",
            "The SSO session associated with this profile has expired or is otherwise invalid. To refresh this SSO session run aws sso login with the corresponding profile.",
            "Error loading SSO Token: Token for https://corp.awsapps.com/start does not exist",
        ] {
            assert!(says_sso_expired(said), "{said}");
        }
        assert!(!says_sso_expired("The config profile (x) could not be found"));
        assert!(says_cli_v1("aws: error: argument operation: Invalid choice, valid choices are: list | get | set"));
        assert_eq!(last_words("warning\n\n  the real reason  \n\n"), "the real reason");
        assert_eq!(export_args("w").join(" "), "configure export-credentials --profile w --format process");
    }

    #[test]
    fn bedrock_profile_process_json_and_expiry() {
        let c = parse_process_json(r#"{"Version": 1, "AccessKeyId": "ASIAX", "SecretAccessKey": "s", "SessionToken": "t", "Expiration": "2026-10-09T15:30:00Z"}"#, "x").unwrap();
        assert_eq!((c.id.as_str(), c.secret.as_str(), c.session.as_str()), ("ASIAX", "s", "t"));
        let exp = c.expires.unwrap();
        assert_eq!(exp, 1_791_559_800);
        assert!(c.fresh(exp - 301) && !c.fresh(exp - 300) && !c.fresh(exp + 10), "refreshed five minutes early");
        let forever = parse_process_json(r#"{"Version":1,"AccessKeyId":"A","SecretAccessKey":"S"}"#, "x").unwrap();
        assert_eq!((forever.expires, forever.session.as_str()), (None, ""));
        assert!(forever.fresh(u64::MAX - 1), "no Expiration, no expiry");
        assert!(parse_process_json(r#"{"Version":2,"AccessKeyId":"A","SecretAccessKey":"S"}"#, "x").unwrap_err().contains("Version"));
        let e = parse_process_json(r#"{"Version":1,"AccessKeyId":"SECRETID"}"#, "the AWS CLI").unwrap_err();
        assert!(e.starts_with("the AWS CLI did not print") && !e.contains("SECRETID"), "{e}");
        assert!(parse_process_json("SECRET garbage", "x").unwrap_err().contains("not JSON"));
        assert!(!format!("{c:?}").contains("ASIAX"), "Debug never shows a key");
    }

    #[test]
    fn bedrock_profile_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-09T15:30:00.123Z"), Some(1_791_559_800));
        assert_eq!(parse_time("2026-10-09T17:30:00+02:00"), Some(1_791_559_800));
        assert_eq!(parse_time("2026-10-09T10:30:00-0500"), Some(1_791_559_800));
        assert_eq!(parse_time("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(parse_time("tomorrow"), None);
        assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn bedrock_profile_command_lines_split_like_the_aws_cli() {
        assert_eq!(split_command("\"/opt/my tools/creds\" --account 'dev team' plain").unwrap(), ["/opt/my tools/creds", "--account", "dev team", "plain"]);
        assert_eq!(split_command("  a\\ b  \"q\\\"x\" 'it''s'  ").unwrap(), ["a b", "q\"x", "its"]);
        assert_eq!(split_command("\"\" x").unwrap(), ["", "x"]);
        assert!(split_command("'open").is_err() && split_command("   ").is_err());
        // No shell: these reach the program as they are.
        assert_eq!(split_command("p $HOME ~ a|b").unwrap(), ["p", "$HOME", "~", "a|b"]);
    }

    #[test]
    fn bedrock_profile_cli_is_looked_for_on_path_then_common_places() {
        assert_eq!(
            cli_candidates(Some("/u/bin:/usr/local/bin/:")),
            ["/u/bin/aws", "/usr/local/bin/aws", "/opt/homebrew/bin/aws", "/usr/bin/aws"]
        );
        assert_eq!(cli_candidates(None), ["/opt/homebrew/bin/aws", "/usr/local/bin/aws", "/usr/bin/aws"]);
        assert!(valid_profile_name("dev-team.admin@corp") && !valid_profile_name("a b") && !valid_profile_name("x]"));
    }
}
