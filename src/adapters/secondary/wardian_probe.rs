//! Asks an address whether a Wardian answers there (ADR-2610080930): a GET of `/api/status` with a
//! short timeout, counted as Wardian only when the answer has Wardian's shape. Used when the usual
//! port is taken: the same Wardian is opened, another one is named and left alone.

use serde_json::Value;
use std::time::Duration;

/// Long enough for a Wardian on this machine, short enough that a program that holds the port
/// and never answers delays the start by under a second.
const TIMEOUT: Duration = Duration::from_millis(800);

/// A Wardian found on a port: its version (None before 0.4.3, which did not report one) and the
/// apps folder it serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub version: Option<String>,
    pub local_root: String,
}

/// The Wardian that answers `/api/status` at `addr` (host:port), if one does.
pub fn wardian_at(addr: &str) -> Option<Found> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).redirects(0).build();
    let v: Value = agent.get(&format!("http://{addr}/api/status")).call().ok()?.into_json().ok()?;
    found(&v)
}

fn found(v: &Value) -> Option<Found> {
    looks_like_wardian(v).then(|| Found { version: v["version"].as_str().map(str::to_string), local_root: v["local_root"].as_str().unwrap_or_default().to_string() })
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

    #[test]
    fn start_reads_the_version_and_folder_of_a_wardian_found() {
        let new = found(&json!({"source": "local", "version": "0.4.3", "local_root": "/w/apps", "apps": 16, "first_run": false})).unwrap();
        assert_eq!((new.version.as_deref(), new.local_root.as_str()), (Some("0.4.3"), "/w/apps"));
        let old = found(&json!({"source": "local", "local_root": "data/apps", "apps": 0, "first_run": false})).unwrap();
        assert_eq!(old.version, None, "a Wardian before 0.4.3 reports no version");
        assert_eq!(found(&json!({"status": "ok"})), None);
    }
}
