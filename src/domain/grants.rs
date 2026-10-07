//! The user's answers to permission questions (SPEC.md 6.9): may a package send or read a
//! channel, or use a host capability such as Splunk. The list is kept as JSON so the browser and
//! the settings file share one shape: [{app, channel, mode, allow, at}].

use super::package::safe_segment;
use serde_json::{json, Value};

/// Capabilities the host provides itself, which the user allows per package.
const HOST_CAPS: &[&str] = &["splunk", "ai"];

/// A channel name: lowercase letters, digits, '.', '-', '_' (SPEC.md §6.9).
pub fn valid_channel(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

/// One answer, from the settings page or the permission bar.
pub struct Answer<'a> {
    pub app: &'a str,
    pub channel: &'a str,
    pub mode: &'a str,
    /// "allow" or "deny", or "ask" to forget the answer.
    pub decision: &'a str,
}

impl<'a> Answer<'a> {
    pub fn from_json(body: &'a Value) -> Answer<'a> {
        let s = |k: &str| body[k].as_str().unwrap_or("");
        Answer { app: s("app"), channel: s("channel"), mode: s("mode"), decision: s("decision") }
    }
}

/// The list with `a` applied: the old answer for the same app, channel and mode replaced.
pub fn apply(mut list: Vec<Value>, a: &Answer, now: u64) -> Result<Vec<Value>, String> {
    if !safe_segment(a.app) {
        return Err("not an app name".into());
    }
    if !valid_channel(a.channel) {
        return Err("not a channel name".into());
    }
    // "use" is an answer about a host capability (the "channel" is its name), not a channel.
    let ok_mode = matches!(a.mode, "send" | "receive") || (a.mode == "use" && HOST_CAPS.contains(&a.channel));
    if !ok_mode || !matches!(a.decision, "allow" | "deny" | "ask") {
        return Err("mode must be send, receive, or use (for a host capability); decision must be allow, deny or ask".into());
    }
    list.retain(|g| !(g["app"] == a.app && g["channel"] == a.channel && g["mode"] == a.mode));
    if a.decision != "ask" {
        list.push(json!({ "app": a.app, "channel": a.channel, "mode": a.mode, "allow": a.decision == "allow", "at": now }));
    }
    Ok(list)
}

/// True when the list says the user allowed `app` this mode on this channel or capability.
pub fn granted(list: &[Value], app: &str, channel: &str, mode: &str) -> bool {
    list.iter().any(|g| g["app"] == app && g["channel"] == channel && g["mode"] == mode && g["allow"] == true)
}

/// True when app `app` of the suite declares capability `cap` in suite.json.
pub fn declares_cap(suite: &Value, app: &str, cap: &str) -> bool {
    suite["apps"].as_array().is_some_and(|apps| apps.iter().any(|a| a["name"] == app && a["caps"].as_array().is_some_and(|c| c.iter().any(|c| c == cap))))
}
