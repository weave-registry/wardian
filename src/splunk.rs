//! Splunk searches for apps that declare the "splunk" capability.
//!
//! Apps never see the Splunk address or credentials: the kernel page asks
//! this server, and this server runs the search with the account saved in
//! Settings (or from SPLUNK_* variables). The account's Splunk role decides
//! what an app can read, so give Wardian a read-only role.

use crate::hub::write_private;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// The most rows one search returns to an app.
const MAX_ROWS: usize = 10_000;
/// The longest Wardian waits for one search before it cancels the job.
const MAX_SEARCH_TIME: Duration = Duration::from_secs(15 * 60);
const MAX_SEARCH_CHARS: usize = 10_000;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Config {
    /// The management address, e.g. https://splunk.example.com:8089
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// Accept any certificate. For Splunk's own self-signed default certificate.
    #[serde(default)]
    pub insecure_tls: bool,
    /// A PEM file of extra certificate authorities to trust.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca_file: String,
}

impl Config {
    fn from_env() -> Option<Config> {
        let url = env("SPLUNK_URL")?;
        Some(Config {
            url,
            token: env("SPLUNK_TOKEN").unwrap_or_default(),
            username: env("SPLUNK_USERNAME").unwrap_or_default(),
            password: env("SPLUNK_PASSWORD").unwrap_or_default(),
            insecure_tls: env("SPLUNK_INSECURE_TLS").is_some_and(|v| v == "1" || v == "true"),
            ca_file: env("SPLUNK_CA_FILE").unwrap_or_default(),
        })
    }

    fn check(&mut self) -> Result<(), String> {
        self.url = base_url(&self.url);
        self.token = self.token.trim().to_string();
        self.username = self.username.trim().to_string();
        if !(self.url.starts_with("https://") || self.url.starts_with("http://")) {
            return Err("the Splunk address must start with https:// (the management port is usually 8089)".into());
        }
        if self.token.is_empty() && (self.username.is_empty() || self.password.is_empty()) {
            return Err("give a Splunk token, or a username and password".into());
        }
        Ok(())
    }

    fn auth(&self) -> String {
        if !self.token.is_empty() {
            format!("Bearer {}", self.token)
        } else {
            format!("Basic {}", base64(format!("{}:{}", self.username, self.password).as_bytes()))
        }
    }
}

pub struct Splunk {
    path: PathBuf,
    cfg: Mutex<Option<(Config, &'static str)>>,
}

impl Splunk {
    pub fn new(data_dir: &Path) -> Splunk {
        let path = data_dir.join("splunk.json");
        let saved = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Config>(&s).ok());
        let cfg = match saved {
            Some(c) => Some((c, "settings")),
            None => Config::from_env().map(|c| (c, "environment")),
        };
        Splunk { path, cfg: Mutex::new(cfg) }
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
            let _ = fs::remove_file(&self.path);
            *self.cfg.lock().unwrap() = Config::from_env().map(|c| (c, "environment"));
            return Ok(self.status());
        }
        let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
        let mut cfg = Config {
            url: url.into(),
            token: s("token"),
            username: s("username"),
            password: s("password"),
            insecure_tls: body["insecure_tls"].as_bool().unwrap_or(false),
            ca_file: s("ca_file"),
        };
        cfg.check()?;
        let info = server_info(&cfg)?;
        let bytes = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
        write_private(&self.path, &bytes).map_err(|e| format!("saving the Splunk settings: {e}"))?;
        *self.cfg.lock().unwrap() = Some((cfg, "settings"));
        let mut st = self.status();
        st["server"] = info;
        Ok(st)
    }

    /// Runs one search and returns its rows: `{fields: [..], rows: [[..]], truncated}`.
    ///
    /// A search over weeks of data can take minutes. Holding one connection
    /// open that long breaks: proxies and load balancers cut it, and the reply
    /// arrives half-read ("error while decoding chunks"). So this starts a
    /// search job, asks every second whether it is done, then reads the rows.
    pub fn search(&self, spl: &str, earliest: &str, latest: &str) -> Result<Value, String> {
        let cfg = self.cfg.lock().unwrap().as_ref().map(|(c, _)| c.clone());
        let cfg = cfg.ok_or("Splunk is not set up on this Wardian: Settings → Splunk")?;
        let spl = normalize_search(spl)?;
        let agent = agent(&cfg)?;
        let jobs = format!("{}/services/search/jobs", cfg.url);
        let read = |r: ureq::Response, url: &str| -> Result<Value, String> {
            r.into_json().map_err(|e| format!("{url}: Splunk's reply was cut off or unreadable ({e})"))
        };

        // "fast" mode: Splunk reads only the fields the search uses. The default can
        // pull every field of every event, which makes a search over weeks many times slower.
        let mut form: Vec<(&str, &str)> = vec![("search", &spl), ("output_mode", "json"), ("adhoc_search_level", "fast")];
        if !earliest.is_empty() {
            form.push(("earliest_time", earliest));
        }
        if !latest.is_empty() {
            form.push(("latest_time", latest));
        }
        let created = agent.post(&jobs).set("Authorization", &cfg.auth()).send_form(&form).map_err(|e| splunk_error(e, "POST", &jobs))?;
        let sid = read(created, &jobs)?["sid"].as_str().map(String::from).ok_or("Splunk did not start the search (no job id)")?;
        let job = format!("{jobs}/{sid}");

        let started = Instant::now();
        loop {
            thread::sleep(Duration::from_millis(if started.elapsed() < Duration::from_secs(5) { 300 } else { 1000 }));
            let st = agent.get(&job).query("output_mode", "json").set("Authorization", &cfg.auth()).call().map_err(|e| splunk_error(e, "GET", &job))?;
            let st = read(st, &job)?;
            let c = &st["entry"][0]["content"];
            if c["isFailed"].as_bool() == Some(true) || c["dispatchState"] == "FAILED" {
                let why = c["messages"].as_array().map(|m| m.iter().filter_map(|m| m["text"].as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default();
                return Err(format!("the Splunk search failed{}", if why.is_empty() { String::new() } else { format!(": {why}") }));
            }
            if c["isDone"].as_bool() == Some(true) || c["dispatchState"] == "DONE" {
                break;
            }
            if started.elapsed() > MAX_SEARCH_TIME {
                let _ = agent.post(&format!("{job}/control")).set("Authorization", &cfg.auth()).send_form(&[("action", "cancel")]);
                return Err(format!("the search ran longer than {} minutes, so Wardian stopped it. Try a shorter time range.", MAX_SEARCH_TIME.as_secs() / 60));
            }
        }

        let results = format!("{job}/results");
        let resp = agent
            .get(&results)
            .query("output_mode", "json")
            .query("count", "10001") // one more than we keep, to know when to say "truncated"
            .set("Authorization", &cfg.auth())
            .call()
            .map_err(|e| splunk_error(e, "GET", &results))?;
        let body = read(resp, &results)?;
        let mut out = table(&body);
        out["seconds"] = json!(started.elapsed().as_secs());
        Ok(out)
    }
}

/// Splunk needs a search to start with a command. Most people type the
/// filter only ("index=x ..."), which means "search index=x ...".
fn normalize_search(spl: &str) -> Result<String, String> {
    let spl = spl.trim();
    if spl.is_empty() {
        return Err("the search is empty".into());
    }
    if spl.chars().count() > MAX_SEARCH_CHARS {
        return Err(format!("a search may be at most {MAX_SEARCH_CHARS} characters"));
    }
    let lower = spl.to_ascii_lowercase();
    Ok(if spl.starts_with('|') || lower.starts_with("search ") { spl.to_string() } else { format!("search {spl}") })
}

/// Turns a oneshot reply into columns and rows, in the order the search
/// named its fields. Internal fields (_raw, _time …) are left out unless the
/// search keeps nothing else. A multivalue cell keeps its first value.
fn table(body: &Value) -> Value {
    let mut fields: Vec<String> = body["fields"]
        .as_array()
        .map(|a| a.iter().filter_map(|f| f["name"].as_str().or_else(|| f.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    let results = body["results"].as_array().cloned().unwrap_or_default();
    if fields.is_empty() {
        if let Some(first) = results.first().and_then(Value::as_object) {
            fields = first.keys().cloned().collect();
        }
    }
    let visible: Vec<String> = fields.iter().filter(|f| !f.starts_with('_')).cloned().collect();
    if !visible.is_empty() {
        fields = visible;
    }
    let truncated = results.len() > MAX_ROWS;
    let rows: Vec<Value> = results
        .iter()
        .take(MAX_ROWS)
        .map(|r| {
            Value::Array(
                fields
                    .iter()
                    .map(|f| match &r[f] {
                        Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
                        v => v.clone(),
                    })
                    .collect(),
            )
        })
        .collect();
    let messages: Vec<Value> = body["messages"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["text"].as_str().map(|t| json!(t))).collect())
        .unwrap_or_default();
    json!({ "fields": fields, "rows": rows, "truncated": truncated, "messages": messages })
}

/// Tests the account and names the server. server/info answers anyone on
/// some Splunk servers, so the account is tested on current-context, which
/// needs a login and says whose it is.
fn server_info(cfg: &Config) -> Result<Value, String> {
    let get = |path: &str| -> Result<Value, String> {
        let url = format!("{}/services/{path}", cfg.url);
        agent(cfg)?
            .get(&url)
            .query("output_mode", "json")
            .set("Authorization", &cfg.auth())
            .call()
            .map_err(|e| splunk_error(e, "GET", &url))?
            .into_json()
            .map_err(|e| format!("{url} answered, but not like Splunk's management port: {e}"))
    };
    let who = get("authentication/current-context")?;
    let user = who["entry"][0]["content"]["username"].clone();
    let info = get("server/info").unwrap_or(Value::Null);
    let c = &info["entry"][0]["content"];
    Ok(json!({ "name": c["serverName"], "version": c["version"], "user": user }))
}

/// The address people paste is often the web UI ("https://host:8000/en-US/app/search")
/// or ends in "/services". Keep only scheme, host and port: every call adds its own path.
fn base_url(url: &str) -> String {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else { return url.trim_end_matches('/').to_string() };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{host}")
}

/// A full reason for a failed Splunk call: what Wardian asked for, what came
/// back, Splunk's own words, and a hint for the usual causes. Also printed in
/// the server log, which has room for the whole reply.
fn splunk_error(e: ureq::Error, method: &str, url: &str) -> String {
    let msg = match e {
        ureq::Error::Status(code, resp) => {
            let server = resp.header("Server").unwrap_or("").to_string();
            let body = resp.into_string().unwrap_or_default();
            eprintln!("splunk: {method} {url} -> {code} (server: {server})\n{}", clip(&body, 4000));
            let said = reply_text(&body);
            let hint = match code {
                401 => " The token or password is wrong or expired. A token also needs token authentication turned on in Splunk (Settings → Tokens).".to_string(),
                403 => " The account has no right to do this. Its role needs the search capability.".to_string(),
                404 => hint_404(url, &server, &body),
                _ => String::new(),
            };
            format!(
                "{method} {url} returned {code}{}{}.{hint}",
                if server.is_empty() { String::new() } else { format!(" from \"{server}\"") },
                if said.is_empty() { String::new() } else { format!(": {said}") },
            )
        }
        ureq::Error::Transport(t) => {
            let text = t.to_string();
            eprintln!("splunk: {method} {url} -> {text}");
            if text.contains("UnsupportedCertVersion") {
                format!("{url} uses an old-format (X.509 version 1) certificate, such as Splunk's default SplunkServerDefaultCert. Tick \"Allow a self-signed certificate\", or ask the Splunk admin for a proper certificate.")
            } else if text.contains("UnknownIssuer") || text.contains("NotValidForName") || text.contains("certificate") {
                format!("cannot trust the certificate of {url} ({text}). Give the CA file, or tick \"Allow a self-signed certificate\".")
            } else if text.contains("InvalidContentType") || text.contains("CorruptMessage") || text.contains("record") {
                format!("{url}: the TLS handshake failed ({text}). If this port speaks plain HTTP, use http:// instead of https://.")
            } else {
                format!("cannot reach {url}: {text}. Check the host name, the port (8089), and that a firewall lets this machine in.")
            }
        }
    };
    msg
}

fn hint_404(url: &str, server: &str, body: &str) -> String {
    let port = url.split("://").nth(1).and_then(|r| r.split('/').next()).and_then(|h| h.rsplit_once(':')).map(|(_, p)| p);
    let lower = body.to_ascii_lowercase();
    if port == Some("8000") || lower.contains("<html") && !server.to_ascii_lowercase().contains("splunkd") {
        " This looks like a web page, not Splunk's REST API. Use the management port, usually https://<host>:8089, not the web port 8000.".into()
    } else if port == Some("8088") || lower.contains("hec") {
        " Port 8088 is the HTTP Event Collector, which only takes data in. Searches need the management port, usually 8089.".into()
    } else if url.contains("splunkcloud.com") {
        " On Splunk Cloud, REST API access must be turned on for your stack, and the address is https://<stack>.splunkcloud.com:8089.".into()
    } else {
        " Check that the address is Splunk's management port (usually 8089) and that no proxy in between rewrites the path.".into()
    }
}

/// Splunk's own words from a reply: messages[].text in JSON or <msg> in XML,
/// or else the reply's text without markup.
fn reply_text(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        let texts: Vec<&str> = v["messages"].as_array().map(|a| a.iter().filter_map(|m| m["text"].as_str()).collect()).unwrap_or_default();
        if !texts.is_empty() {
            return texts.join("; ");
        }
    }
    if let Some(start) = body.find("<msg") {
        if let Some(open_end) = body[start..].find('>') {
            let rest = &body[start + open_end + 1..];
            if let Some(close) = rest.find("</msg>") {
                return rest[..close].trim().to_string();
            }
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    clip(&out.split_whitespace().collect::<Vec<_>>().join(" "), 300)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

fn agent(cfg: &Config) -> Result<ureq::Agent, String> {
    // No limit on a whole request: a slow reply is fine as long as data keeps coming.
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .timeout_write(Duration::from_secs(60));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    if cfg.insecure_tls {
        let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCert(provider)))
            .with_no_client_auth();
        b = b.tls_config(Arc::new(tls));
    } else if !cfg.ca_file.is_empty() {
        let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let certs = CertificateDer::pem_file_iter(&cfg.ca_file).map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?;
        for cert in certs {
            roots.add(cert.map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?).map_err(|e| e.to_string())?;
        }
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        b = b.tls_config(Arc::new(tls));
    }
    Ok(b.build())
}

/// Trusts any certificate. Only used when the admin ticks "allow a
/// self-signed certificate".
///
/// It also skips the handshake signature check. That check reads the
/// certificate with webpki, which refuses X.509 version 1, and Splunk's own
/// default certificate (SplunkServerDefaultCert) is version 1. Skipping it
/// loses nothing here: a verifier that accepts any certificate already
/// accepts an attacker's, and the attacker can sign with that one.
#[derive(Debug)]
struct AnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_search_command() {
        assert_eq!(normalize_search("index=lt | stats count").unwrap(), "search index=lt | stats count");
        assert_eq!(normalize_search("  | inputlookup x.csv").unwrap(), "| inputlookup x.csv");
        assert_eq!(normalize_search("Search index=lt").unwrap(), "Search index=lt");
        assert!(normalize_search("  ").is_err());
    }

    #[test]
    fn table_keeps_field_order_and_drops_internal_fields() {
        let body = json!({
            "fields": [{"name": "concurrency"}, {"name": "x"}, {"name": "_time"}, {"name": "r"}],
            "results": [
                {"concurrency": "1", "x": "980", "_time": "t", "r": ["1.0", "1.1"]},
                {"concurrency": "2", "x": "1900"}
            ],
            "messages": [{"type": "INFO", "text": "hello"}]
        });
        let t = table(&body);
        assert_eq!(t["fields"], json!(["concurrency", "x", "r"]));
        assert_eq!(t["rows"], json!([["1", "980", "1.0"], ["2", "1900", null]]));
        assert_eq!(t["truncated"], json!(false));
        assert_eq!(t["messages"], json!(["hello"]));
    }

    #[test]
    fn table_without_fields_list() {
        let t = table(&json!({"results": [{"_raw": "a"}]}));
        assert_eq!(t["fields"], json!(["_raw"]));
    }

    #[test]
    fn keeps_only_scheme_host_and_port() {
        assert_eq!(base_url("https://splunk:8089/"), "https://splunk:8089");
        assert_eq!(base_url("https://splunk:8089/services"), "https://splunk:8089");
        assert_eq!(base_url(" https://splunk:8000/en-US/app/search/search?q=x "), "https://splunk:8000");
    }

    #[test]
    fn reads_splunk_words_from_any_reply() {
        assert_eq!(reply_text(r#"{"messages":[{"type":"ERROR","text":"Not Found"}]}"#), "Not Found");
        assert_eq!(reply_text("<response><messages><msg type=\"ERROR\">Unknown endpoint.</msg></messages></response>"), "Unknown endpoint.");
        assert_eq!(reply_text("<html><body><h1>404</h1> Page not found</body></html>"), "404 Page not found");
    }

    #[test]
    fn hints_for_wrong_ports() {
        assert!(hint_404("https://h:8000/services/server/info", "", "<html>").contains("8089"));
        assert!(hint_404("https://h:8088/services/server/info", "", "").contains("Event Collector"));
        assert!(hint_404("https://x.splunkcloud.com:8089/services/server/info", "Splunkd", "").contains("Splunk Cloud"));
    }

    #[test]
    fn basic_auth_header() {
        assert_eq!(base64(b"admin:changeme"), "YWRtaW46Y2hhbmdlbWU=");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn config_needs_https_and_credentials() {
        let mut c = Config { url: "splunk:8089".into(), token: "t".into(), ..Default::default() };
        assert!(c.check().is_err());
        c.url = "https://splunk:8089/".into();
        c.check().unwrap();
        assert_eq!(c.url, "https://splunk:8089");
        c.token.clear();
        c.username = "u".into();
        assert!(c.check().is_err());
    }
}
