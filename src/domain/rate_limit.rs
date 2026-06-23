use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const PUBLIC_WEBHOOK_EXECUTION_LIMIT: u32 = 5;
pub const PUBLIC_WEBHOOK_EXECUTION_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitDecision {
    pub bucket: String,
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset_after_seconds: u64,
}

pub struct FixedWindowRateLimiter {
    limit: u32,
    window: Duration,
    state: Mutex<HashMap<String, BucketState>>,
}

#[derive(Clone, Debug)]
struct BucketState {
    started_at: Instant,
    count: u32,
}

impl FixedWindowRateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    pub fn public_webhook_execution() -> Self {
        Self::new(
            PUBLIC_WEBHOOK_EXECUTION_LIMIT,
            PUBLIC_WEBHOOK_EXECUTION_WINDOW,
        )
    }

    pub fn check(&self, bucket: impl Into<String>) -> RateLimitDecision {
        let bucket = bucket.into();
        let now = Instant::now();
        let mut state = self
            .state
            .lock()
            .expect("rate limiter mutex should not be poisoned");
        let bucket_state = state.entry(bucket.clone()).or_insert(BucketState {
            started_at: now,
            count: 0,
        });

        if now.duration_since(bucket_state.started_at) >= self.window {
            bucket_state.started_at = now;
            bucket_state.count = 0;
        }

        let reset_after_seconds = self
            .window
            .saturating_sub(now.duration_since(bucket_state.started_at))
            .as_secs()
            .max(1);

        if bucket_state.count >= self.limit {
            return RateLimitDecision {
                bucket,
                allowed: false,
                limit: self.limit,
                remaining: 0,
                reset_after_seconds,
            };
        }

        bucket_state.count += 1;
        RateLimitDecision {
            bucket,
            allowed: true,
            limit: self.limit,
            remaining: self.limit.saturating_sub(bucket_state.count),
            reset_after_seconds,
        }
    }
}
