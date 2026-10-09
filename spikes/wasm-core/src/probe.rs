//! Probes the browser calls, each a real Wardian function on in-memory input. A probe that
//! returns ran; one that panics traps (`unreachable`), which is the finding.

use crate::browser::{run_queued, BrowserClock, QueueTasks};
use crate::domain::{check, suite};
use crate::ports::clock::Clock;
use crate::ports::service::{Builder, JobKind, Jobs, Searches, Watch};
use crate::usecases::jobs::JobRunner;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// `wardian check` on a minimal module app, held in memory. Returns the error count.
#[no_mangle]
pub extern "C" fn probe_check() -> u32 {
    let wasm: &[u8] = b"\0asm\x01\0\0\0";
    let json: &[u8] = br#"{"format":1,"title":"Adder"}"#;
    let files = vec![("app.wasm".to_string(), wasm.len() as u64), ("app.json".to_string(), json.len() as u64)];
    let read = |p: &str| match p {
        "app.wasm" => Some(wasm.to_vec()),
        "app.json" => Some(json.to_vec()),
        _ => None,
    };
    let mut r = check::Report::default();
    check::check_package("adder", &files, &read, &mut r);
    r.errors.len() as u32
}

/// A suite part's frame document, built the way the server builds it. Returns its length.
#[no_mangle]
pub extern "C" fn probe_frame() -> u32 {
    let read = |p: &str| -> Option<Vec<u8>> {
        match p {
            "suite.json" => Some(br#"{"format":1,"title":"S","apps":[{"name":"a","slot":"main"}]}"#.to_vec()),
            "apps/a/view.html" => Some(b"<p>hi</p>".to_vec()),
            "apps/a/app.js" => Some(b"Kernel.register('a',{});".to_vec()),
            _ => None,
        }
    };
    suite::frame(&read, "/* shim */", Some("a")).map(|h| h.len() as u32).unwrap_or(0)
}

/// The clock port's time, from the page's import.
#[no_mangle]
pub extern "C" fn probe_now() -> f64 {
    BrowserClock.now_ms() as f64
}

/// The clock port's sleep: how many milliseconds a sleep of `ms` really took.
#[no_mangle]
pub extern "C" fn probe_sleep(ms: f64) -> f64 {
    let c = BrowserClock;
    let t = c.now_ms();
    c.sleep(Duration::from_secs_f64(ms / 1000.0));
    (c.now_ms() - t) as f64
}

/// A search that polls three times, 100 ms apart, through the clock port, as Splunk's does.
struct PollingSearches;

impl Searches for PollingSearches {
    fn status(&self) -> Value {
        Value::Null
    }
    fn set_config(&self, _: &Value) -> Result<Value, Value> {
        Err(json!({ "error": "no" }))
    }
    fn search(&self, _: &str, _: &str, _: &str, watch: &dyn Watch) -> Result<Value, String> {
        for i in 0..3 {
            BrowserClock.sleep(Duration::from_millis(100));
            watch.progress(i);
        }
        Ok(json!({ "rows": 3 }))
    }
    fn search_into(&self, _: &str, _: &str, _: &str, _: &str, _: &str, _: &dyn Watch) -> Result<Value, String> {
        Err("no".into())
    }
}

struct NoBuilder;

impl Builder for NoBuilder {
    fn status(&self) -> Value { Value::Null }
    fn set_key(&self, _: &str, _: Option<&str>) -> Result<Value, String> { Err("no".into()) }
    fn set_provider(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn send(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn sample(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn stop(&self, _: &str) -> Result<Value, String> { Err("no".into()) }
    fn claim_test(&self, _: &str, _: usize) -> Result<Value, String> { Err("no".into()) }
    fn tested(&self, _: &str, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn sessions(&self) -> Value { Value::Null }
    fn events(&self, _: &str, _: usize) -> Result<Value, String> { Err("no".into()) }
    fn agent(&self) -> Value { Value::Null }
    fn set_agent(&self, _: &Value) -> Result<Value, String> { Err("no".into()) }
    fn usage(&self) -> Value { Value::Null }
    fn aws_profiles(&self) -> Value { Value::Null }
}

/// The real job runner, on the browser's clock and task queue.
fn jobs() -> &'static JobRunner {
    static JOBS: OnceLock<JobRunner> = OnceLock::new();
    JOBS.get_or_init(|| JobRunner::new(Arc::new(PollingSearches), Arc::new(NoBuilder), Arc::new(BrowserClock), Arc::new(QueueTasks)))
}

/// Starts a Splunk search job. Returns 1 when `start` answered with a job id.
#[no_mangle]
pub extern "C" fn job_start() -> u32 {
    jobs().start(JobKind::SplunkSearch, "pkg", "app", &json!({ "search": "index=x" })).map(|v| v["job"].is_string() as u32).unwrap_or(0)
}

/// The newest job's state: 1 running, 2 done, 3 failed, 0 none.
#[no_mangle]
pub extern "C" fn job_state() -> u32 {
    match jobs().list(Some("pkg"))["jobs"][0]["state"].as_str() {
        Some("running") => 1,
        Some("done") => 2,
        Some("failed") => 3,
        _ => 0,
    }
}

/// Runs the queued background work; says how many tasks ran.
#[no_mangle]
pub extern "C" fn run_tasks() -> u32 {
    run_queued()
}
