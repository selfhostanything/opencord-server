use std::env;

use crate::version;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    pub version: String,
    pub public_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            version: version::VERSION.to_owned(),
            public_url: trim_url(
                env::var("OPENCORD_PUBLIC_URL").unwrap_or_else(|_| "http://localhost:8080".into()),
            ),
        }
    }
}

pub fn api_bind_addr() -> String {
    normalize_bind_addr(env::var("OPENCORD_API_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()))
}

pub fn realtime_bind_addr() -> String {
    normalize_bind_addr(
        env::var("OPENCORD_REALTIME_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".into()),
    )
}

pub fn worker_bind_addr() -> String {
    normalize_bind_addr(env::var("OPENCORD_WORKER_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".into()))
}

fn normalize_bind_addr(addr: String) -> String {
    if addr.starts_with(':') {
        format!("0.0.0.0{addr}")
    } else {
        addr
    }
}

fn trim_url(url: String) -> String {
    let trimmed = url.trim().trim_end_matches('/').to_owned();
    if trimmed.is_empty() {
        "http://localhost:8080".to_owned()
    } else {
        trimmed
    }
}
