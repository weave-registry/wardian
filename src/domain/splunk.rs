//! Splunk, as Wardian sees it: the account settings, the search text, and the table a search
//! returns to an app. The calls themselves go through the Splunk port.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// The most rows one search returns to an app.
const MAX_ROWS: usize = 10_000;
/// The longest Wardian waits for one search before it cancels the job.
pub const MAX_SEARCH_TIME: Duration = Duration::from_secs(15 * 60);
const MAX_SEARCH_CHARS: usize = 10_000;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct SplunkConfig {
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

impl SplunkConfig {
    /// Tidies the address and says what is missing.
    pub fn check(&mut self) -> Result<(), String> {
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

    /// The Authorization header value.
    pub fn auth(&self) -> String {
        if !self.token.is_empty() {
            format!("Bearer {}", self.token)
        } else {
            format!("Basic {}", base64(format!("{}:{}", self.username, self.password).as_bytes()))
        }
    }
}

/// Splunk needs a search to start with a command. Most people type the
/// filter only ("index=x ..."), which means "search index=x ...".
pub fn normalize_search(spl: &str) -> Result<String, String> {
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
pub fn table(body: &Value) -> Value {
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

/// The address people paste is often the web UI ("https://host:8000/en-US/app/search")
/// or ends in "/services". Keep only scheme, host and port: every call adds its own path.
fn base_url(url: &str) -> String {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else { return url.trim_end_matches('/').to_string() };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{host}")
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
