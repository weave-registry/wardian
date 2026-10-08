//! The job runner (ADR-2610072118): a long call runs on its own thread, and the browser asks how it
//! is going with short requests instead of holding one connection open for minutes. Jobs live in
//! memory, so a restart forgets them; finished ones are kept an hour, at most 50 per package.

use crate::domain::jobs::{prune, Job, JobKind};
use crate::ports::service::{Builder, Jobs, Searches, Watch};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

/// A running job's cancel flag and progress, shared with the thread that runs it.
#[derive(Default)]
struct Flags {
    stop: AtomicBool,
    /// Rows loaded so far, plus one; 0 means nothing reported yet.
    rows: AtomicU64,
}

impl Watch for Flags {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    fn progress(&self, rows: u64) {
        self.rows.store(rows.saturating_add(1), Ordering::Relaxed);
    }
}

struct Entry {
    job: Job,
    flags: Arc<Flags>,
}

pub struct JobRunner {
    searches: Arc<dyn Searches>,
    builder: Arc<dyn Builder>,
    jobs: Arc<Mutex<Vec<Entry>>>,
    next: AtomicU64,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)).unwrap_or(0)
}

impl JobRunner {
    /// What the composition root shows or logs at start about where jobs are kept.
    pub const NOTE: &'static str = "jobs: background jobs are kept in memory; a restart forgets them";

    pub fn new(searches: Arc<dyn Searches>, builder: Arc<dyn Builder>) -> JobRunner {
        JobRunner { searches, builder, jobs: Arc::new(Mutex::new(Vec::new())), next: AtomicU64::new(1) }
    }

    /// The jobs with each running one's latest progress, after pruning; runs `f` under the lock.
    fn with_jobs<T>(&self, f: impl FnOnce(&mut Vec<Entry>, u64) -> T) -> T {
        let now = now_ms();
        let mut all = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        for e in all.iter_mut().filter(|e| e.job.running()) {
            let rows = e.flags.rows.load(Ordering::Relaxed);
            if rows > 0 {
                e.job.progress = Some(rows - 1);
            }
        }
        let mut list: Vec<Job> = all.iter().map(|e| e.job.clone()).collect();
        prune(&mut list, now);
        all.retain(|e| list.iter().any(|j| j.id == e.job.id));
        f(&mut all, now)
    }

    fn find<'a>(all: &'a mut [Entry], id: &str, package: Option<&str>) -> Result<&'a mut Entry, String> {
        // Another package's job is "not found", the same as one that never was.
        all.iter_mut().find(|e| e.job.id == id && package.is_none_or(|p| p == e.job.package)).ok_or_else(|| format!("no job {id}"))
    }

    fn run(searches: &dyn Searches, builder: &dyn Builder, kind: JobKind, package: &str, body: &Value, watch: &dyn Watch) -> Result<Value, String> {
        let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
        match kind {
            JobKind::SplunkSearch => searches.search(&s("search"), &s("earliest"), &s("latest"), watch),
            JobKind::SplunkInto => {
                let table = Some(s("table")).filter(|x| !x.is_empty()).unwrap_or_else(|| "search".into());
                searches.search_into(package, &table, &s("search"), &s("earliest"), &s("latest"), watch)
            }
            JobKind::AiSample => builder.sample(body),
        }
    }
}

impl Jobs for JobRunner {
    fn start(&self, kind: JobKind, package: &str, app: &str, body: &Value) -> Result<Value, String> {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let started = now_ms();
        let id = format!("j{n}-{:x}", started % 0x10_0000);
        let label = match kind {
            JobKind::AiSample => body["prompt"].as_str().unwrap_or(""),
            _ => body["search"].as_str().unwrap_or(""),
        };
        let flags = Arc::new(Flags::default());
        let job = Job::new(id.clone(), package, app, kind, label, started);
        self.with_jobs(|all, _| all.push(Entry { job, flags: Arc::clone(&flags) }));

        let (searches, builder, jobs) = (Arc::clone(&self.searches), Arc::clone(&self.builder), Arc::clone(&self.jobs));
        let (package, app, body, job_id) = (package.to_string(), app.to_string(), body.clone(), id.clone());
        thread::spawn(move || {
            let out = JobRunner::run(&*searches, &*builder, kind, &package, &body, &*flags);
            println!("jobs: {package}/{app} {} {job_id}: {}", kind.name(), if out.is_ok() { "done" } else { "ended" });
            let mut all = jobs.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(e) = all.iter_mut().find(|e| e.job.id == job_id) {
                e.job.finish(out, now_ms());
            }
        });
        Ok(json!({ "job": id }))
    }

    fn list(&self, package: Option<&str>) -> Value {
        self.with_jobs(|all, now| {
            let jobs: Vec<Value> = all.iter().rev().filter(|e| package.is_none_or(|p| p == e.job.package)).map(|e| e.job.summary(now)).collect();
            json!({ "jobs": jobs })
        })
    }

    fn get(&self, id: &str, package: Option<&str>) -> Result<Value, String> {
        self.with_jobs(|all, now| JobRunner::find(all, id, package).map(|e| e.job.full(now)))
    }

    fn cancel(&self, id: &str, package: Option<&str>) -> Result<Value, String> {
        self.with_jobs(|all, now| {
            let e = JobRunner::find(all, id, package)?;
            // The thread sees the flag at its next poll and cancels the Splunk search job.
            e.flags.stop.store(true, Ordering::Relaxed);
            e.job.cancel(now);
            Ok(e.job.summary(now))
        })
    }
}
