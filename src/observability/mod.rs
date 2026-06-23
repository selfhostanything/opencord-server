use crate::config::{LogFormat, RuntimeConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceContext {
    pub traceparent: String,
}

impl TraceContext {
    pub fn new(traceparent: impl Into<String>) -> Option<Self> {
        let traceparent = traceparent.into();
        if valid_traceparent(traceparent.trim()) {
            Some(Self { traceparent })
        } else {
            None
        }
    }
}

pub fn otel_export_enabled(config: &RuntimeConfig) -> bool {
    config.otel.enabled && config.otel.endpoint.is_some()
}

pub fn init_tracing(config: &RuntimeConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_new(&config.log.env_filter)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let result = match config.log.format {
        LogFormat::Text => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .try_init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .try_init(),
    };

    if result.is_err() {
        tracing::debug!("tracing subscriber was already initialized");
    }
}

fn valid_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 4 {
        return false;
    }

    valid_hex(parts[0], 2, true)
        && parts[0] != "ff"
        && valid_hex(parts[1], 32, false)
        && valid_hex(parts[2], 16, false)
        && valid_hex(parts[3], 2, true)
}

fn valid_hex(value: &str, len: usize, allow_zero: bool) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && (allow_zero || value.bytes().any(|byte| byte != b'0'))
}
