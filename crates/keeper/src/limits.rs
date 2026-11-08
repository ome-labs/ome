use std::collections::HashMap;

/// Token bucket per requester, in points: refills `per_minute / 60` per second up to `per_minute`.
pub(crate) struct RateLimiter {
    per_minute: u32,
    buckets: HashMap<String, (f64, u64)>,
}

impl RateLimiter {
    pub(crate) fn new(per_minute: u32) -> Self {
        Self {
            per_minute,
            buckets: HashMap::new(),
        }
    }

    /// Takes `cost` points for `requester` at `now`; `false` if the budget is exhausted.
    pub(crate) fn take(&mut self, requester: &str, cost: u32, now: u64) -> bool {
        let capacity = f64::from(self.per_minute);
        let (tokens, last) = self
            .buckets
            .entry(requester.to_owned())
            .or_insert((capacity, now));
        #[allow(clippy::cast_precision_loss)]
        let elapsed = now.saturating_sub(*last) as f64;
        *tokens = (*tokens + elapsed * capacity / 60.0).min(capacity);
        *last = now;
        if *tokens >= f64::from(cost) {
            *tokens -= f64::from(cost);
            true
        } else {
            false
        }
    }
}
