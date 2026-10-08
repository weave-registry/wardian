//! Asks an address whether a Wardian answers there (ADR-2610080930): a GET of `/api/status` with a
//! short timeout, counted as Wardian only when the answer has Wardian's shape. Used when the usual
//! port is taken, to open the Wardian already running instead of failing.

use serde_json::Value;
use std::time::Duration;

/// Long enough for a Wardian on this machine, short enough that a program that holds the port
/// and never answers delays the start by under a second.
const TIMEOUT: Duration = Duration::from_millis(800);

/// Whether a Wardian answers `/api/status` at `addr` (host:port).
pub fn is_wardian(addr: &str) -> bool {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).redirects(0).build();
    agent
        .get(&format!("http://{addr}/api/status"))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())
        .is_some_and(|v| looks_like_wardian(&v))
}

/// Wardian's `/api/status`: an object with `source` "local" or "drive", the apps folder, the
/// number of apps and whether this is the first run.
fn looks_like_wardian(v: &Value) -> bool {
    matches!(v["source"].as_str(), Some("local" | "drive")) && v["local_root"].is_string() && v["apps"].is_u64() && v["first_run"].is_boolean()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn start_only_wardians_status_counts() {
        assert!(looks_like_wardian(&json!({"source": "local", "local_root": "/a", "apps": 5, "first_run": false, "admin": true})));
        assert!(looks_like_wardian(&json!({"source": "drive", "local_root": "/a", "apps": 0, "first_run": true})));
        assert!(!looks_like_wardian(&json!({"source": "local", "apps": 5, "first_run": false})));
        assert!(!looks_like_wardian(&json!({"source": "web", "local_root": "/a", "apps": 5, "first_run": false})));
        assert!(!looks_like_wardian(&json!({"status": "ok"})));
        assert!(!looks_like_wardian(&json!([1, 2])));
    }
}
