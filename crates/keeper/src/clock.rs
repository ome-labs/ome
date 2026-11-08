use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock source in unix seconds; replaced in tests.
pub trait Clock: Send + Sync {
    /// Current unix time in seconds.
    fn now(&self) -> u64;
}

/// The system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}
