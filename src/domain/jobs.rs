//! Long calls run as background jobs (ADR-2610072118): what a job is, its states, and how long
//! finished jobs are kept. The runner (usecases/jobs.rs) owns the threads and the clock; this
//! module only knows the record and the rules, so every time here is a number passed in.

use serde_json::{json, Value};

/// How long a finished job is kept, in milliseconds: an hour.
const KEEP_FOR_MS: u64 = 60 * 60 * 1000;
/// The most finished jobs kept per package; the oldest go first.
const KEEP_PER_PACKAGE: usize = 50;

/// The calls that can run as a job, and the name each has in the API.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JobKind {
    /// `/api/splunk/search`: a search whose rows come back whole (at most 10,000).
    SplunkSearch,
    /// `/api/db/search-into`: a search loaded into a table of the package's database.
    SplunkInto,
    /// `/api/ai/sample`: claude:sample.
    AiSample,
}

impl JobKind {
    pub fn name(self) -> &'static str {
        match self {
            JobKind::SplunkSearch => "splunk.search",
            JobKind::SplunkInto => "splunk.into",
            JobKind::AiSample => "ai.sample",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    fn name(self) -> &'static str {
        match self {
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// One job: who started it, what it is, and how it went.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub package: String,
    app: String,
    kind: JobKind,
    /// A short label for lists: the search, or the start of the prompt.
    label: String,
    state: JobState,
    /// Rows loaded so far, when the call reports it.
    pub progress: Option<u64>,
    /// Milliseconds since the Unix epoch.
    started: u64,
    ended: Option<u64>,
    result: Option<Value>,
    error: Option<String>,
}

/// The longest label kept, in characters.
const LABEL_CHARS: usize = 120;

impl Job {
    pub fn new(id: String, package: &str, app: &str, kind: JobKind, label: &str, now: u64) -> Job {
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = if label.chars().count() > LABEL_CHARS { format!("{}…", label.chars().take(LABEL_CHARS - 1).collect::<String>()) } else { label };
        Job { id, package: package.into(), app: app.into(), kind, label, state: JobState::Running, progress: None, started: now, ended: None, result: None, error: None }
    }

    pub fn running(&self) -> bool {
        self.state == JobState::Running
    }

    /// Ends the job with the call's answer. A job already ended (cancelled) stays as it is.
    pub fn finish(&mut self, out: Result<Value, String>, now: u64) {
        if !self.running() {
            return;
        }
        match out {
            Ok(v) => {
                if let Some(n) = v["total"].as_u64() {
                    self.progress = Some(n);
                }
                self.state = JobState::Done;
                self.result = Some(v);
            }
            Err(e) => {
                self.state = JobState::Failed;
                self.error = Some(e);
            }
        }
        self.ended = Some(now);
    }

    /// Marks a running job cancelled; false when it had already ended.
    pub fn cancel(&mut self, now: u64) -> bool {
        if !self.running() {
            return false;
        }
        self.state = JobState::Cancelled;
        self.error = Some("cancelled".into());
        self.ended = Some(now);
        true
    }

    /// The job as lists show it, without its result.
    pub fn summary(&self, now: u64) -> Value {
        json!({
            "id": self.id, "package": self.package, "app": self.app, "kind": self.kind.name(), "label": self.label,
            "state": self.state.name(), "progress": self.progress, "started": self.started, "ended": self.ended,
            "elapsed": self.ended.unwrap_or(now).saturating_sub(self.started), "error": self.error,
        })
    }

    /// The job with its result, once it is done.
    pub fn full(&self, now: u64) -> Value {
        let mut v = self.summary(now);
        v["result"] = self.result.clone().unwrap_or(Value::Null);
        v
    }
}

/// Drops finished jobs older than KEEP_FOR_MS, then all but the newest KEEP_PER_PACKAGE finished
/// jobs of each package. Running jobs are always kept. `jobs` is in the order they started.
pub fn prune(jobs: &mut Vec<Job>, now: u64) {
    jobs.retain(|j| j.running() || j.ended.is_some_and(|e| now.saturating_sub(e) < KEEP_FOR_MS));
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut keep = vec![true; jobs.len()];
    for (i, j) in jobs.iter().enumerate().rev() {
        if j.running() {
            continue;
        }
        let at = match seen.iter().position(|(p, _)| *p == j.package) {
            Some(at) => at,
            None => {
                seen.push((j.package.clone(), 0));
                seen.len() - 1
            }
        };
        seen[at].1 += 1;
        keep[i] = seen[at].1 <= KEEP_PER_PACKAGE;
    }
    let mut it = keep.into_iter();
    jobs.retain(|_| it.next().unwrap_or(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(n: usize, package: &str, at: u64) -> Job {
        Job::new(format!("j{n}"), package, "table", JobKind::SplunkInto, "index=x", at)
    }

    #[test]
    fn a_job_runs_then_ends_once() {
        let mut j = job(1, "p", 1000);
        assert!(j.running());
        assert_eq!(j.summary(1500)["elapsed"], 500);
        j.finish(Ok(json!({"total": 42})), 2000);
        assert_eq!(j.summary(9000)["state"], "done");
        assert_eq!(j.progress, Some(42), "a load's total is its last progress");
        assert_eq!(j.summary(9000)["elapsed"], 1000, "time stops when the job ends");
        assert!(!j.cancel(3000), "a finished job cannot be cancelled");
        assert_eq!(j.full(9000)["result"]["total"], 42);
        assert!(j.summary(9000).get("result").is_none(), "lists leave the result out");

        let mut c = job(2, "p", 0);
        assert!(c.cancel(10));
        c.finish(Ok(json!({})), 20);
        assert_eq!(c.summary(30)["state"], "cancelled", "an answer after a cancel is dropped");
        let mut f = job(3, "p", 0);
        f.finish(Err("no".into()), 5);
        assert_eq!(f.summary(30)["state"], "failed");
        assert_eq!(f.summary(30)["error"], "no");
        assert_eq!(JobKind::AiSample.name(), "ai.sample");
        assert_eq!(JobKind::SplunkSearch.name(), "splunk.search");
    }

    #[test]
    fn labels_are_short_and_on_one_line() {
        let j = Job::new("j".into(), "p", "a", JobKind::SplunkSearch, &format!("index=x\n  | {}", "y".repeat(500)), 0);
        assert!(j.label.starts_with("index=x | y") && j.label.chars().count() == LABEL_CHARS && j.label.ends_with('…'));
    }

    #[test]
    fn finished_jobs_are_kept_an_hour_and_fifty_per_package() {
        let mut jobs: Vec<Job> = (0..60).map(|n| { let mut j = job(n, "a", 0); j.finish(Ok(json!({})), 10); j }).collect();
        jobs.push(job(60, "a", 0)); // still running
        let mut old = job(61, "b", 0);
        old.finish(Ok(json!({})), 0);
        jobs.push(old);
        prune(&mut jobs, KEEP_FOR_MS - 1);
        assert_eq!(jobs.iter().filter(|j| j.package == "a" && !j.running()).count(), KEEP_PER_PACKAGE);
        assert_eq!(jobs.iter().filter(|j| j.package == "a" && !j.running()).map(|j| j.id.as_str()).next(), Some("j10"), "the oldest go first");
        assert!(jobs.iter().any(|j| j.id == "j60"), "a running job is kept");
        assert!(jobs.iter().any(|j| j.id == "j61"), "b's job is younger than an hour");
        prune(&mut jobs, KEEP_FOR_MS + 10);
        assert!(jobs.iter().all(|j| j.running()), "after an hour only the running job is left");
        assert_eq!(jobs.len(), 1);
    }
}
