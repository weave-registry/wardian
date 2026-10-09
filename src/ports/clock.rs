//! Time and background work (ADR-2610091040). The core never reads the system clock or starts a
//! thread itself: both trap when it runs in a browser, and neither can be set by a test.

use std::time::Duration;

/// The time, a wait, and numbers for names that only need to differ.
pub trait Clock: Send + Sync {
    /// Milliseconds since 1970-01-01 UTC.
    fn now_ms(&self) -> u64;
    /// Seconds since 1970-01-01 UTC.
    fn now(&self) -> u64 {
        self.now_ms() / 1000
    }
    /// Waits for `d`.
    fn sleep(&self, d: Duration);
    /// A number that differs from call to call, for staging folders and chat ids. Not a secret.
    fn nonce(&self) -> u32;
}

/// Work that runs in the background, after `spawn` returns.
pub trait Tasks: Send + Sync {
    fn spawn(&self, task: Box<dyn FnOnce() + Send>);
}
