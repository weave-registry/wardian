//! AWS profiles for Amazon Bedrock (ADR-2610091530): which profiles there are, where the AWS CLI
//! is, and the keys of one profile, fetched when a request needs them and kept in memory only,
//! until five minutes before they expire.
//!
//! With AWS CLI v2, `aws configure export-credentials --profile <name> --format process` signs in
//! for every kind of profile (keys, SSO, roles, MFA, `credential_process`). Without it, Wardian
//! reads a profile's keys itself or runs its `credential_process`; an SSO or role profile then
//! needs the CLI. What a program prints is never logged or repeated: it holds the keys.

use crate::domain::aws_profile::{self as aws, AwsFiles, Creds, Source};
use crate::ports::{
    clock::Clock,
    llm::{AwsCredentials, AwsKeys},
    programs::{Programs, RunError},
    storage::FileSystem,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

/// What the environment says about AWS, read once by the composition root.
#[derive(Clone, Default)]
pub struct AwsEnv {
    pub home: Option<PathBuf>,
    /// AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE, when set.
    pub config_file: Option<PathBuf>,
    pub credentials_file: Option<PathBuf>,
    /// PATH, where `aws` is looked for first.
    pub path: Option<String>,
    /// WARDIAN_AWS_CLI: the AWS CLI to use, or `none` to read profiles without it.
    pub cli: Option<String>,
}

pub struct AwsProfiles {
    fs: Arc<dyn FileSystem>,
    programs: Arc<dyn Programs>,
    clock: Arc<dyn Clock>,
    env: AwsEnv,
    /// Keys by profile name; never written anywhere.
    kept: Mutex<HashMap<String, Creds>>,
}

/// The files as read, owned.
struct Read {
    config_path: String,
    config: Option<String>,
    credentials_path: String,
    credentials: Option<String>,
}

impl Read {
    fn files(&self) -> AwsFiles<'_> {
        AwsFiles { config_path: &self.config_path, config: self.config.as_deref(), credentials_path: &self.credentials_path, credentials: self.credentials.as_deref() }
    }
}

impl AwsProfiles {
    pub fn new(fs: Arc<dyn FileSystem>, programs: Arc<dyn Programs>, clock: Arc<dyn Clock>, env: AwsEnv) -> AwsProfiles {
        AwsProfiles { fs, programs, clock, env, kept: Mutex::new(HashMap::new()) }
    }

    fn read(&self) -> Read {
        let aws_dir = self.env.home.clone().unwrap_or_default().join(".aws");
        let config = self.env.config_file.clone().unwrap_or_else(|| aws_dir.join("config"));
        let credentials = self.env.credentials_file.clone().unwrap_or_else(|| aws_dir.join("credentials"));
        let text = |p: &PathBuf| self.fs.read(p).map(|b| String::from_utf8_lossy(&b).into_owned());
        Read { config: text(&config), config_path: config.display().to_string(), credentials: text(&credentials), credentials_path: credentials.display().to_string() }
    }

    /// For Settings: the profiles' names and regions (never a key), the files read, and the AWS
    /// CLI found, if any.
    pub fn list(&self) -> Value {
        let read = self.read();
        let profiles: Vec<Value> = aws::profile_names(&read.files()).into_iter().map(|p| json!({ "name": p.name, "region": p.region })).collect();
        json!({ "profiles": profiles, "files": [read.config_path, read.credentials_path], "cli": self.find_cli() })
    }

    /// The AWS CLI's full path: WARDIAN_AWS_CLI, else the first `aws` on PATH or in the usual
    /// places. None with WARDIAN_AWS_CLI=none, or when there is none.
    pub fn find_cli(&self) -> Option<String> {
        match self.env.cli.as_deref() {
            Some("none") => None,
            Some(path) if !path.is_empty() => Some(path.to_string()),
            _ => aws::cli_candidates(self.env.path.as_deref()).into_iter().find(|c| self.fs.is_file(std::path::Path::new(c))),
        }
    }

    /// The profile's region; Err when there is no such profile.
    pub fn region_of(&self, name: &str) -> Result<Option<String>, String> {
        let read = self.read();
        aws::resolve(&read.files(), name).map(|p| p.region)
    }

    /// Fetches a profile's keys now: through the AWS CLI when there is one, else from the files.
    fn fetch(&self, name: &str, cli: &str) -> Result<Creds, String> {
        let read = self.read();
        let profile = aws::resolve(&read.files(), name);
        // A CLI recorded at save time that has since gone is looked for again.
        let cli = Some(cli.to_string()).filter(|c| !c.is_empty() && self.env.cli.as_deref() != Some("none") && self.fs.is_file(std::path::Path::new(c))).or_else(|| self.find_cli());
        if let Some(cli) = cli {
            // The CLI reads the files itself, but a name in neither file is worth saying plainly.
            profile?;
            return self.from_cli(name, &cli);
        }
        match profile?.source {
            Source::Keys(c) => Ok(c),
            Source::Process(line) => self.from_process(name, &line),
            Source::NeedsCli(why) => Err(aws::needs_cli(name, why)),
        }
    }

    fn run(&self, program: &str, args: &[String], what: &str) -> Result<crate::ports::programs::Output, String> {
        match self.programs.run(program, args, Duration::from_secs(aws::RUN_TIMEOUT_SECS)) {
            Ok(out) => Ok(out),
            Err(RunError::TimedOut) => Err(format!("{what} did not finish within {} seconds", aws::RUN_TIMEOUT_SECS)),
            Err(RunError::NotStarted(why)) => Err(format!("could not run {what}: {why}")),
        }
    }

    fn from_cli(&self, name: &str, cli: &str) -> Result<Creds, String> {
        let what = format!("`aws configure export-credentials --profile {name}`");
        let out = self.run(cli, &aws::export_args(name), &what)?;
        if out.code != Some(0) {
            if aws::says_sso_expired(&out.stderr) {
                return Err(aws::sso_expired(name));
            }
            if aws::says_cli_v1(&out.stderr) {
                return Err(format!("the AWS CLI at {cli} is version 1, which cannot hand over a profile's keys: install AWS CLI v2"));
            }
            return Err(format!("{what} failed: {}", aws::last_words(&out.stderr)));
        }
        aws::parse_process_json(&out.stdout, "the AWS CLI")
    }

    fn from_process(&self, name: &str, line: &str) -> Result<Creds, String> {
        let words = aws::split_command(line).map_err(|e| format!("AWS profile \"{name}\": {e}"))?;
        let what = format!("the credential_process of AWS profile \"{name}\"");
        let out = self.run(&words[0], &words[1..], &what)?;
        if out.code != Some(0) {
            return Err(format!("{what} failed: {}", aws::last_words(&out.stderr)));
        }
        aws::parse_process_json(&out.stdout, &what)
    }
}

impl AwsCredentials for AwsProfiles {
    fn keys(&self, profile: &str, cli: &str) -> Result<AwsKeys, String> {
        let now = self.clock.now();
        let kept = self.kept.lock().unwrap().get(profile).filter(|c| c.fresh(now)).cloned();
        let creds = match kept {
            Some(c) => c,
            None => {
                let c = self.fetch(profile, cli)?;
                self.kept.lock().unwrap().insert(profile.to_string(), c.clone());
                c
            }
        };
        Ok(AwsKeys { id: creds.id, secret: creds.secret, session: creds.session })
    }

    fn forget(&self, profile: &str) {
        self.kept.lock().unwrap().remove(profile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::secondary::local_disk::LocalDisk;
    use crate::ports::programs::Output;

    /// Programs that answer from a script: (program, first arg) → output; records each run.
    struct Fake {
        runs: Mutex<Vec<String>>,
        answer: Box<dyn Fn(&str, &[String]) -> Result<Output, RunError> + Send + Sync>,
    }
    impl Programs for Fake {
        fn run(&self, program: &str, args: &[String], timeout: Duration) -> Result<Output, RunError> {
            assert_eq!(timeout, Duration::from_secs(30));
            self.runs.lock().unwrap().push(format!("{program} {}", args.join(" ")));
            (self.answer)(program, args)
        }
    }
    struct Clock1(Mutex<u64>);
    impl Clock for Clock1 {
        fn now_ms(&self) -> u64 {
            *self.0.lock().unwrap() * 1000
        }
        fn sleep(&self, _: Duration) {}
        fn nonce(&self) -> u32 {
            0
        }
    }

    fn ok(stdout: &str) -> Result<Output, RunError> {
        Ok(Output { code: Some(0), stdout: stdout.into(), stderr: String::new() })
    }

    struct Setup {
        _dir: tempfile_dir::Dir,
        profiles: AwsProfiles,
        fake: Arc<Fake>,
        clock: Arc<Clock1>,
    }

    mod tempfile_dir {
        pub struct Dir(pub std::path::PathBuf);
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn setup(config: &str, credentials: &str, cli: Option<&str>, answer: impl Fn(&str, &[String]) -> Result<Output, RunError> + Send + Sync + 'static) -> Setup {
        let dir = std::env::temp_dir().join(format!("wardian-aws-{}-{:?}", std::process::id(), std::thread::current().id()).replace(['(', ')'], ""));
        std::fs::create_dir_all(dir.join(".aws")).unwrap();
        std::fs::write(dir.join(".aws/config"), config).unwrap();
        std::fs::write(dir.join(".aws/credentials"), credentials).unwrap();
        let fake = Arc::new(Fake { runs: Mutex::new(Vec::new()), answer: Box::new(answer) });
        let clock = Arc::new(Clock1(Mutex::new(1_000_000)));
        let env = AwsEnv { home: Some(dir.clone()), cli: Some(cli.unwrap_or("none").to_string()), ..AwsEnv::default() };
        let profiles = AwsProfiles::new(Arc::new(LocalDisk), fake.clone(), clock.clone(), env);
        Setup { _dir: tempfile_dir::Dir(dir), profiles, fake, clock }
    }

    #[test]
    fn bedrock_profile_keys_are_kept_until_five_minutes_before_they_expire() {
        let s = setup("[profile p]\nregion = eu-west-1\ncredential_process = /bin/creds --for 'my team'\n", "", None, |_, _| {
            ok(r#"{"Version":1,"AccessKeyId":"ASIA1","SecretAccessKey":"S1","SessionToken":"T1","Expiration":"1970-01-12T14:00:00Z"}"#)
        });
        // 1970-01-12T14:00:00Z is 1_000_800: keys are fresh until 1_000_500.
        let k = s.profiles.keys("p", "").unwrap();
        assert_eq!((k.id.as_str(), k.secret.as_str(), k.session.as_str()), ("ASIA1", "S1", "T1"));
        s.profiles.keys("p", "").unwrap();
        assert_eq!(s.fake.runs.lock().unwrap().len(), 1, "kept in memory");
        *s.clock.0.lock().unwrap() = 1_000_500;
        s.profiles.keys("p", "").unwrap();
        assert_eq!(*s.fake.runs.lock().unwrap(), ["/bin/creds --for my team", "/bin/creds --for my team"], "fetched again five minutes early, run without a shell");
        s.profiles.forget("p");
        s.profiles.keys("p", "").unwrap();
        assert_eq!(s.fake.runs.lock().unwrap().len(), 3, "forget drops them");
        assert_eq!(s.profiles.region_of("p").unwrap().as_deref(), Some("eu-west-1"));
        let listed = s.profiles.list();
        assert_eq!(listed["profiles"], json!([{ "name": "p", "region": "eu-west-1" }]));
        assert!(!listed.to_string().contains("ASIA1") && listed["cli"].is_null());
    }

    #[test]
    fn bedrock_profile_the_cli_signs_in_when_there_is_one() {
        let s = setup("[profile work]\nsso_session = corp\nregion = us-east-1\n", "", Some("/x/aws"), |_, args| {
            assert_eq!(args.join(" "), "configure export-credentials --profile work --format process");
            ok(r#"{"Version":1,"AccessKeyId":"ASIACLI","SecretAccessKey":"S","SessionToken":"T"}"#)
        });
        // WARDIAN_AWS_CLI names it, so a saved path that is gone is replaced by it.
        assert_eq!(s.profiles.keys("work", "/gone/aws").unwrap().id, "ASIACLI");
        assert_eq!(*s.fake.runs.lock().unwrap(), ["/x/aws configure export-credentials --profile work --format process"]);
        // Without Expiration the keys do not expire.
        *s.clock.0.lock().unwrap() = u64::MAX / 2000;
        s.profiles.keys("work", "").unwrap();
        assert_eq!(s.fake.runs.lock().unwrap().len(), 1);
        assert!(s.profiles.keys("nope", "").unwrap_err().starts_with("there is no AWS profile \"nope\" in "));
    }

    #[test]
    fn bedrock_profile_failures_say_what_to_do() {
        let fail = |stderr: &'static str| move |_: &str, _: &[String]| Ok(Output { code: Some(255), stdout: "SECRET-STDOUT".into(), stderr: stderr.into() });
        let s = setup("[profile work]\nsso_session = corp\n", "", Some("/x/aws"), fail("\nError when retrieving token from sso: Token has expired and refresh failed\n"));
        assert_eq!(s.profiles.keys("work", "").unwrap_err(), "the AWS sign-in of profile \"work\" has expired. Run `aws sso login --profile work`, then try again");
        let s = setup("[profile work]\nsso_session = corp\n", "", Some("/x/aws"), fail("aws: error: argument operation: Invalid choice, valid choices are:"));
        assert!(s.profiles.keys("work", "").unwrap_err().contains("version 1"));
        let s = setup("[profile work]\nsso_session = corp\n", "", Some("/x/aws"), |_, _| Err(RunError::TimedOut));
        assert_eq!(s.profiles.keys("work", "").unwrap_err(), "`aws configure export-credentials --profile work` did not finish within 30 seconds");
        // Without the CLI, SSO and roles need it; nothing is run.
        let s = setup("[profile work]\nsso_session = corp\n[profile role]\nrole_arn = arn:aws:iam::1:role/r\n", "", None, |_, _| panic!("nothing runs"));
        assert!(s.profiles.keys("work", "").unwrap_err().contains("signs in through IAM Identity Center (SSO), which needs AWS CLI v2"));
        assert!(s.profiles.keys("role", "").unwrap_err().contains("assumes a role (role_arn), which needs AWS CLI v2"));
        // A credential_process that fails: its last words, never its output.
        let s = setup("[profile p]\ncredential_process = /bin/creds\n", "", None, fail("creds: not signed in"));
        let e = s.profiles.keys("p", "").unwrap_err();
        assert_eq!(e, "the credential_process of AWS profile \"p\" failed: creds: not signed in");
        // Keys written in the credentials file need nothing run.
        let s = setup("[profile k]\nregion = us-east-1\n", "[k]\naws_access_key_id = AKIAFILE\naws_secret_access_key = file secret\n", None, |_, _| panic!("nothing runs"));
        assert_eq!(s.profiles.keys("k", "").unwrap().secret, "file secret");
    }
}
