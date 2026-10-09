//! The system's clock and threads, behind the `Clock` and `Tasks` ports (ADR-2610091040).

use crate::ports::clock::{Clock, Tasks};
use std::{
    hash::{BuildHasher, Hasher},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)).unwrap_or(0)
    }
    fn sleep(&self, d: Duration) {
        thread::sleep(d)
    }
    /// From the standard library's per-process random hash keys and the time, so it needs no
    /// system call.
    fn nonce(&self) -> u32 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        u32::try_from(h.finish() & 0xffff_ffff).unwrap_or(0)
    }
}

/// Each task on its own thread.
pub struct Threads;

impl Tasks for Threads {
    fn spawn(&self, task: Box<dyn FnOnce() + Send>) {
        thread::spawn(task);
    }
}
