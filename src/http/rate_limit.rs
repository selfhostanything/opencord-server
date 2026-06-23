use axum::http::{HeaderMap, HeaderValue, header};

use crate::domain::rate_limit::RateLimitDecision;

pub fn rate_limit_headers(decision: &RateLimitDecision) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-ratelimit-limit",
        HeaderValue::from_str(&decision.limit.to_string()).expect("valid rate limit header"),
    );
    headers.insert(
        "x-ratelimit-remaining",
        HeaderValue::from_str(&decision.remaining.to_string()).expect("valid rate limit header"),
    );
    headers.insert(
        "x-ratelimit-reset",
        HeaderValue::from_str(&decision.reset_after_seconds.to_string())
            .expect("valid rate limit header"),
    );
    headers.insert(
        "x-ratelimit-bucket",
        HeaderValue::from_str(&decision.bucket).expect("valid rate limit header"),
    );
    if !decision.allowed {
        headers.insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&decision.reset_after_seconds.to_string())
                .expect("valid retry-after header"),
        );
    }

    headers
}
