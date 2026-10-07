//! Splunk searches for apps that declare the "splunk" capability.
//!
//! Apps never see the Splunk address or credentials: the kernel page asks
//! this server, and this server runs the search with the account saved in
//! Settings (or from SPLUNK_* variables). The account's Splunk role decides
//! what an app can read, so give Wardian a read-only role.

use crate::domain::splunk::{fields_of, normalize_search, rows_of, table, SplunkConfig, MAX_SEARCH_TIME};
use crate::ports::{
    db::{column_names, column_types, valid_ident, Database, MAX_LOADED_ROWS},
    service::Searches,
    splunk::{SplunkApi, SplunkError, SplunkSession},
    storage::FileSystem,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub struct Splunk {
    fs: Arc<dyn FileSystem>,
    api: Arc<dyn SplunkApi>,
    path: PathBuf,
    /// The account from the environment, used when Settings has none.
    env_cfg: Option<SplunkConfig>,
    cfg: Mutex<Option<(SplunkConfig, &'static str)>>,
    /// Where a search's results go when they are loaded into a table (ADR-2610071219).
    db: Arc<dyn Database>,
}

/// Rows read from Splunk per request when loading a table.
const CHUNK: usize = 50_000;

/// A failed call as the user reads it; `unreadable` says what to call a reply that is not JSON.
fn said(e: SplunkError, unreadable: impl Fn(&str, &str) -> String) -> String {
    match e {
        SplunkError::Call(text) => text,
        SplunkError::Unreadable { url, reason } => unreadable(&url, &reason),
    }
}

fn cut_off(url: &str, reason: &str) -> String {
    format!("{url}: Splunk's reply was cut off or unreadable ({reason})")
}

impl Splunk {
    pub fn new(fs: Arc<dyn FileSystem>, api: Arc<dyn SplunkApi>, db: Arc<dyn Database>, data_dir: &Path, env_cfg: Option<SplunkConfig>) -> Splunk {
        let path = data_dir.join("splunk.json");
        let saved = fs.read(&path).and_then(|b| serde_json::from_slice::<SplunkConfig>(&b).ok());
        let cfg = match saved {
            Some(c) => Some((c, "settings")),
            None => env_cfg.clone().map(|c| (c, "environment")),
        };
        Splunk { fs, api, path, env_cfg, cfg: Mutex::new(cfg), db }
    }

    /// What Settings shows. Never the token or the password.
    pub fn status(&self) -> Value {
        match &*self.cfg.lock().unwrap() {
            Some((c, from)) => json!({
                "ready": true, "url": c.url, "from": from, "insecure_tls": c.insecure_tls,
                "auth": if c.token.is_empty() { format!("user {}", c.username) } else { "token".to_string() },
            }),
            None => json!({ "ready": false }),
        }
    }

    /// Tests the account against Splunk before saving it, so a bad setting
    /// never replaces a good one. An empty address removes the saved one.
    pub fn set_config(&self, body: &Value) -> Result<Value, String> {
        let url = body["url"].as_str().unwrap_or("").trim();
        if url.is_empty() {
            self.fs.remove_file(&self.path);
            *self.cfg.lock().unwrap() = self.env_cfg.clone().map(|c| (c, "environment"));
            return Ok(self.status());
        }
        let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
        let mut cfg = SplunkConfig {
            url: url.into(),
            token: s("token"),
            username: s("username"),
            password: s("password"),
            insecure_tls: body["insecure_tls"].as_bool().unwrap_or(false),
            ca_file: s("ca_file"),
        };
        cfg.check()?;
        let info = self.server_info(&cfg)?;
        let bytes = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
        self.fs.write_private(&self.path, &bytes).map_err(|e| format!("saving the Splunk settings: {e}"))?;
        *self.cfg.lock().unwrap() = Some((cfg, "settings"));
        let mut st = self.status();
        st["server"] = info;
        Ok(st)
    }

    /// Tests the account and names the server. server/info answers anyone on
    /// some Splunk servers, so the account is tested on current-context, which
    /// needs a login and says whose it is.
    fn server_info(&self, cfg: &SplunkConfig) -> Result<Value, String> {
        let session = self.api.connect(cfg)?;
        let get = |path: &str| {
            session.get(&format!("/services/{path}"), &[("output_mode", "json")]).map_err(|e| said(e, |url, why| format!("{url} answered, but not like Splunk's management port: {why}")))
        };
        let who = get("authentication/current-context")?;
        let user = who["entry"][0]["content"]["username"].clone();
        let info = get("server/info").unwrap_or(Value::Null);
        let c = &info["entry"][0]["content"];
        Ok(json!({ "name": c["serverName"], "version": c["version"], "user": user }))
    }

    /// Runs one search and returns its rows: `{fields: [..], rows: [[..]], truncated}`.
    ///
    /// A search over weeks of data can take minutes. Holding one connection
    /// open that long breaks: proxies and load balancers cut it, and the reply
    /// arrives half-read ("error while decoding chunks"). So this starts a
    /// search job, asks every second whether it is done, then reads the rows.
    pub fn search(&self, spl: &str, earliest: &str, latest: &str) -> Result<Value, String> {
        let (session, job, started) = self.run_job(spl, earliest, latest)?;
        // One more row than is kept, to know when to say "truncated".
        let body = session.get(&format!("{job}/results"), &[("output_mode", "json"), ("count", "10001")]).map_err(|e| said(e, cut_off))?;
        let mut out = table(&body);
        out["seconds"] = json!(started.elapsed().as_secs());
        Ok(out)
    }

    /// Runs a search and loads every result, up to MAX_LOADED_ROWS, into `table` of `package`'s
    /// database, reading CHUNK rows per request (ADR-2610071219).
    pub fn search_into(&self, package: &str, table_name: &str, spl: &str, earliest: &str, latest: &str) -> Result<Value, String> {
        if !valid_ident(table_name) {
            return Err(format!("\"{table_name}\" is not a table name (letters, digits and _)"));
        }
        let (session, job, started) = self.run_job(spl, earliest, latest)?;
        let mut fields: Vec<String> = Vec::new();
        let mut columns: Vec<String> = Vec::new();
        let mut total = 0usize;
        let mut messages: Vec<Value> = Vec::new();
        let mut truncated = false;
        loop {
            let want = CHUNK.min(MAX_LOADED_ROWS - total);
            let (offset, count) = (total.to_string(), want.to_string());
            let body = session
                .get(&format!("{job}/results"), &[("output_mode", "json"), ("count", &count), ("offset", &offset)])
                .map_err(|e| said(e, cut_off))?;
            if total == 0 {
                fields = fields_of(&body);
                if fields.is_empty() {
                    return Ok(json!({ "table": table_name, "columns": [], "fields": [], "total": 0, "truncated": false, "seconds": started.elapsed().as_secs(), "messages": [] }));
                }
                columns = column_names(&fields);
                let sample: Vec<Vec<Value>> = rows_of(&body, &fields).into_iter().take(500).collect();
                self.db.create_table(package, table_name, &columns, &column_types(columns.len(), &sample), true)?;
                messages = body["messages"].as_array().map(|a| a.iter().filter_map(|m| m["text"].as_str().map(|t| json!(t))).collect()).unwrap_or_default();
            }
            let rows = rows_of(&body, &fields);
            let got = rows.len();
            if got > 0 {
                self.db.insert_rows(package, table_name, &columns, &rows)?;
            }
            total += got;
            if got < want {
                break;
            }
            if total >= MAX_LOADED_ROWS {
                truncated = true;
                break;
            }
        }
        Ok(json!({ "table": table_name, "columns": columns, "fields": fields, "total": total, "truncated": truncated, "seconds": started.elapsed().as_secs(), "messages": messages }))
    }

    /// Starts a search job and waits until it is done. A search over weeks of data can take
    /// minutes. Holding one connection open that long breaks: proxies and load balancers cut it,
    /// and the reply arrives half-read ("error while decoding chunks"). So this starts a job, asks
    /// every second whether it is done, and leaves reading the rows to the caller.
    fn run_job(&self, spl: &str, earliest: &str, latest: &str) -> Result<(Box<dyn SplunkSession>, String, Instant), String> {
        let cfg = self.cfg.lock().unwrap().as_ref().map(|(c, _)| c.clone());
        let cfg = cfg.ok_or("Splunk is not set up on this Wardian: Settings → Splunk")?;
        let spl = normalize_search(spl)?;
        let session = self.api.connect(&cfg)?;
        let jobs = "/services/search/jobs";

        // "fast" mode: Splunk reads only the fields the search uses. The default can
        // pull every field of every event, which makes a search over weeks many times slower.
        let mut form: Vec<(&str, &str)> = vec![("search", &spl), ("output_mode", "json"), ("adhoc_search_level", "fast")];
        if !earliest.is_empty() {
            form.push(("earliest_time", earliest));
        }
        if !latest.is_empty() {
            form.push(("latest_time", latest));
        }
        let created = session.post(jobs, &form).map_err(|e| said(e, cut_off))?;
        let sid = created["sid"].as_str().map(String::from).ok_or("Splunk did not start the search (no job id)")?;
        let job = format!("{jobs}/{sid}");

        let started = Instant::now();
        loop {
            thread::sleep(Duration::from_millis(if started.elapsed() < Duration::from_secs(5) { 300 } else { 1000 }));
            let st = session.get(&job, &[("output_mode", "json")]).map_err(|e| said(e, cut_off))?;
            let c = &st["entry"][0]["content"];
            if c["isFailed"].as_bool() == Some(true) || c["dispatchState"] == "FAILED" {
                let why = c["messages"].as_array().map(|m| m.iter().filter_map(|m| m["text"].as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default();
                return Err(format!("the Splunk search failed{}", if why.is_empty() { String::new() } else { format!(": {why}") }));
            }
            if c["isDone"].as_bool() == Some(true) || c["dispatchState"] == "DONE" {
                return Ok((session, job, started));
            }
            if started.elapsed() > MAX_SEARCH_TIME {
                cancel(&*session, &job);
                return Err(format!("the search ran longer than {} minutes, so Wardian stopped it. Try a shorter time range.", MAX_SEARCH_TIME.as_secs() / 60));
            }
        }
    }
}

/// Stops a job; its reply does not matter.
fn cancel(session: &dyn SplunkSession, job: &str) {
    let _ = session.post(&format!("{job}/control"), &[("action", "cancel")]);
}

impl Searches for Splunk {
    fn status(&self) -> Value {
        Splunk::status(self)
    }
    fn set_config(&self, body: &Value) -> Result<Value, String> {
        Splunk::set_config(self, body)
    }
    fn search(&self, spl: &str, earliest: &str, latest: &str) -> Result<Value, String> {
        Splunk::search(self, spl, earliest, latest)
    }
    fn search_into(&self, package: &str, table: &str, spl: &str, earliest: &str, latest: &str) -> Result<Value, String> {
        Splunk::search_into(self, package, table, spl, earliest, latest)
    }
}
