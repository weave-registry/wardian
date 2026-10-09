//! What a browser adapter for the clock and tasks ports would be (ADR-2610091040). The page that
//! loads the module supplies two imports: the time, and a wait that blocks (`Atomics.wait` in a
//! dedicated worker). Tasks queue up and run when the page calls `run_tasks`, one at a time,
//! because a worker has one thread.

use crate::ports::clock::{Clock, Tasks};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[link(wasm_import_module = "env")]
extern "C" {
    fn now_ms() -> f64;
    fn sleep_ms(ms: f64);
}

pub struct BrowserClock;

static NONCE: AtomicU32 = AtomicU32::new(0);

impl Clock for BrowserClock {
    fn now_ms(&self) -> u64 {
        unsafe { now_ms() as u64 }
    }
    fn sleep(&self, d: Duration) {
        unsafe { sleep_ms(d.as_secs_f64() * 1000.0) }
    }
    fn nonce(&self) -> u32 {
        NONCE.fetch_add(1, Ordering::Relaxed) ^ (self.now_ms() as u32)
    }
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Box<dyn FnOnce() + Send>>> = RefCell::new(VecDeque::new());
}

/// Background work, queued; `run_queued` runs it.
pub struct QueueTasks;

impl Tasks for QueueTasks {
    fn spawn(&self, task: Box<dyn FnOnce() + Send>) {
        QUEUE.with(|q| q.borrow_mut().push_back(task));
    }
}

/// Runs every queued task, in order, and says how many ran.
pub fn run_queued() -> u32 {
    let mut n = 0;
    while let Some(task) = QUEUE.with(|q| q.borrow_mut().pop_front()) {
        task();
        n += 1;
    }
    n
}
