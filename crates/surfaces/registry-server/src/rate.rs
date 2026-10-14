//! Fixed-window in-memory rate limiter (spec #21, T3).
//!
//! Keys are opaque strings built by callers (e.g. `ip:{addr}` or
//! `token:{hash-prefix}`). Counters reset every `window_secs`. State is
//! in-memory: limits are per-process, which matches the single-node registry
//! story and keeps the slice small.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
    window: Duration,
}

#[derive(Debug)]
struct Bucket {
    count: u32,
    window_start: Instant,
}

/// A refused request carries how long until the caller may retry.
#[derive(Debug, Clone, Copy)]
pub struct Limited {
    pub retry_after: Duration,
}

impl RateLimiter {
    pub fn new(window_secs: u64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            window: Duration::from_secs(window_secs),
        }
    }

    /// Allow one request under `limit` per window for `key`.
    /// Returns `Err(Limited)` when the caller is over budget.
    pub fn check(&self, key: &str, limit: u32) -> Result<(), Limited> {
        let mut buckets = self.buckets.lock();
        let now = Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            count: 0,
            window_start: now,
        });

        if now.duration_since(bucket.window_start) >= self.window {
            bucket.count = 0;
            bucket.window_start = now;
        }

        if bucket.count >= limit {
            let elapsed = now.duration_since(bucket.window_start);
            return Err(Limited {
                retry_after: self.window - elapsed,
            });
        }
        bucket.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_refuses() {
        let limiter = RateLimiter::new(60);
        for _ in 0..3 {
            assert!(limiter.check("ip:a", 3).is_ok());
        }
        assert!(limiter.check("ip:a", 3).is_err());
    }

    #[test]
    fn keys_are_independent() {
        let limiter = RateLimiter::new(60);
        assert!(limiter.check("ip:a", 1).is_ok());
        assert!(limiter.check("ip:a", 1).is_err());
        // different key unaffected
        assert!(limiter.check("ip:b", 1).is_ok());
        // token key independent of ip key
        assert!(limiter.check("token:x", 1).is_ok());
    }

    #[test]
    fn window_reset_restores_budget() {
        let limiter = RateLimiter::new(1);
        assert!(limiter.check("ip:c", 1).is_ok());
        assert!(limiter.check("ip:c", 1).is_err());
        std::thread::sleep(Duration::from_millis(1100));
        assert!(limiter.check("ip:c", 1).is_ok());
    }
}
