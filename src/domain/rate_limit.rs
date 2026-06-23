use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

pub const AUTH_LOGIN_LIMIT: u32 = 5;
pub const AUTH_LOGIN_WINDOW: Duration = Duration::from_secs(60);
pub const AUTH_REGISTER_LIMIT: u32 = 5;
pub const AUTH_REGISTER_WINDOW: Duration = Duration::from_secs(60);
pub const ATTACHMENT_PRESIGN_LIMIT: u32 = 5;
pub const ATTACHMENT_PRESIGN_WINDOW: Duration = Duration::from_secs(60);
pub const ATTACHMENT_UPLOAD_LIMIT: u32 = 10;
pub const ATTACHMENT_UPLOAD_WINDOW: Duration = Duration::from_secs(60);
pub const COMPAT_REST_BOT_LIMIT: u32 = 10;
pub const COMPAT_REST_BOT_WINDOW: Duration = Duration::from_secs(60);
pub const COMPAT_GATEWAY_IDENTIFY_LIMIT: u32 = 1;
pub const COMPAT_GATEWAY_IDENTIFY_WINDOW: Duration = Duration::from_secs(5);
pub const MESSAGE_CREATE_LIMIT: u32 = 5;
pub const MESSAGE_CREATE_WINDOW: Duration = Duration::from_secs(60);
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

    pub fn auth_register() -> Self {
        Self::new(AUTH_REGISTER_LIMIT, AUTH_REGISTER_WINDOW)
    }

    pub fn auth_login() -> Self {
        Self::new(AUTH_LOGIN_LIMIT, AUTH_LOGIN_WINDOW)
    }

    pub fn message_create() -> Self {
        Self::new(MESSAGE_CREATE_LIMIT, MESSAGE_CREATE_WINDOW)
    }

    pub fn attachment_presign() -> Self {
        Self::new(ATTACHMENT_PRESIGN_LIMIT, ATTACHMENT_PRESIGN_WINDOW)
    }

    pub fn attachment_upload() -> Self {
        Self::new(ATTACHMENT_UPLOAD_LIMIT, ATTACHMENT_UPLOAD_WINDOW)
    }

    pub fn compat_rest_bot() -> Self {
        Self::new(COMPAT_REST_BOT_LIMIT, COMPAT_REST_BOT_WINDOW)
    }

    pub fn compat_gateway_identify() -> Self {
        Self::new(
            COMPAT_GATEWAY_IDENTIFY_LIMIT,
            COMPAT_GATEWAY_IDENTIFY_WINDOW,
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

pub fn compat_rest_bot_bucket(application_id: Uuid) -> String {
    format!("compat-rest:bot:{application_id}")
}

pub fn compat_gateway_identify_bucket(application_id: Uuid) -> String {
    format!("compat-gateway:identify:{application_id}")
}

pub fn auth_register_bucket(email: &str) -> String {
    format!("auth:register:{}", normalized_email_key(email))
}

pub fn auth_login_bucket(email: &str) -> String {
    format!("auth:login:{}", normalized_email_key(email))
}

pub fn message_create_bucket(user_id: Uuid, channel_id: Uuid) -> String {
    format!("message:create:{user_id}:{channel_id}")
}

pub fn attachment_presign_bucket(user_id: Uuid, channel_id: Uuid) -> String {
    format!("attachment:presign:{user_id}:{channel_id}")
}

pub fn attachment_upload_bucket(user_id: Uuid) -> String {
    format!("attachment:upload:{user_id}")
}

fn normalized_email_key(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}
