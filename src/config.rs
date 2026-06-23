use std::env;
use std::fmt;

use crate::version;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    pub version: String,
    pub public_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub public_url: String,
    pub bind: ServiceBindConfig,
    pub database: DatabaseConfig,
    pub kafka: KafkaConfig,
    pub scylla: ScyllaConfig,
    pub valkey: ValkeyConfig,
    pub object_storage: ObjectStorageConfig,
    pub otel: OtelConfig,
    pub log: LogConfig,
    pub metrics: MetricsConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBindConfig {
    pub api: String,
    pub realtime: String,
    pub worker: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseConfig {
    pub url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KafkaConfig {
    pub bootstrap_servers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScyllaConfig {
    pub contact_points: Vec<String>,
    pub keyspace: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValkeyConfig {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectStorageConfig {
    pub endpoint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtelConfig {
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub service_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogConfig {
    pub format: LogFormat,
    pub env_filter: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricsConfig {
    pub prometheus_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

type EnvLookup<'a> = dyn Fn(&str) -> Option<String> + 'a;

const DEFAULT_PUBLIC_URL: &str = "http://localhost:8080";
const DEFAULT_API_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_REALTIME_ADDR: &str = "0.0.0.0:8081";
const DEFAULT_WORKER_ADDR: &str = "0.0.0.0:8082";
const DEFAULT_KAFKA_BOOTSTRAP_SERVERS: &str = "localhost:29092";
const DEFAULT_SCYLLA_CONTACT_POINTS: &str = "localhost:9042";
const DEFAULT_SCYLLA_KEYSPACE: &str = "opencord";
const DEFAULT_VALKEY_URL: &str = "redis://localhost:6379/0";
const DEFAULT_OBJECT_STORAGE_ENDPOINT: &str = "http://localhost:9000";
const DEFAULT_OTEL_SERVICE_NAME: &str = "opencord-server";
const DEFAULT_LOG_FILTER: &str = "opencord_server=info,info";

impl AppConfig {
    pub fn from_env() -> Self {
        let public_url =
            env::var("OPENCORD_PUBLIC_URL").unwrap_or_else(|_| DEFAULT_PUBLIC_URL.into());

        Self {
            version: version::VERSION.to_owned(),
            public_url: trim_url(public_url),
        }
    }

    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self {
            version: version::VERSION.to_owned(),
            public_url: runtime.public_url.clone(),
        }
    }
}

impl RuntimeConfig {
    pub fn from_env() -> Self {
        Self::try_from_env().expect("OpenCord runtime configuration is invalid")
    }

    pub fn try_from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(&|key| env::var(key).ok())
    }

    pub fn from_env_pairs(pairs: &[(&str, &str)]) -> Result<Self, ConfigError> {
        Self::from_lookup(&|key| {
            pairs
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    fn from_lookup(lookup: &EnvLookup<'_>) -> Result<Self, ConfigError> {
        Ok(Self {
            public_url: trim_url(value_or_default(
                lookup,
                "OPENCORD_PUBLIC_URL",
                DEFAULT_PUBLIC_URL,
            )),
            bind: ServiceBindConfig {
                api: normalize_bind_addr(value_or_default(
                    lookup,
                    "OPENCORD_API_ADDR",
                    DEFAULT_API_ADDR,
                )),
                realtime: normalize_bind_addr(value_or_default(
                    lookup,
                    "OPENCORD_REALTIME_ADDR",
                    DEFAULT_REALTIME_ADDR,
                )),
                worker: normalize_bind_addr(value_or_default(
                    lookup,
                    "OPENCORD_WORKER_ADDR",
                    DEFAULT_WORKER_ADDR,
                )),
            },
            database: DatabaseConfig {
                url: lookup("DATABASE_URL").filter(|value| !value.trim().is_empty()),
            },
            kafka: KafkaConfig {
                bootstrap_servers: parse_endpoint_list(
                    value_or_default(
                        lookup,
                        "KAFKA_BOOTSTRAP_SERVERS",
                        DEFAULT_KAFKA_BOOTSTRAP_SERVERS,
                    ),
                    "KAFKA_BOOTSTRAP_SERVERS",
                )?,
            },
            scylla: ScyllaConfig {
                contact_points: parse_endpoint_list(
                    value_or_default(
                        lookup,
                        "SCYLLA_CONTACT_POINTS",
                        DEFAULT_SCYLLA_CONTACT_POINTS,
                    ),
                    "SCYLLA_CONTACT_POINTS",
                )?,
                keyspace: non_empty_value(
                    value_or_default(lookup, "SCYLLA_KEYSPACE", DEFAULT_SCYLLA_KEYSPACE),
                    "SCYLLA_KEYSPACE",
                )?,
            },
            valkey: ValkeyConfig {
                url: non_empty_value(
                    value_or_default(lookup, "VALKEY_URL", DEFAULT_VALKEY_URL),
                    "VALKEY_URL",
                )?,
            },
            object_storage: ObjectStorageConfig {
                endpoint: trim_url(value_or_default(
                    lookup,
                    "S3_ENDPOINT",
                    DEFAULT_OBJECT_STORAGE_ENDPOINT,
                )),
            },
            otel: OtelConfig {
                enabled: parse_bool(value_or_default(lookup, "OPENCORD_OTEL_ENABLED", "false"))?,
                endpoint: lookup("OPENCORD_OTEL_ENDPOINT").filter(|value| !value.trim().is_empty()),
                service_name: non_empty_value(
                    value_or_default(
                        lookup,
                        "OPENCORD_OTEL_SERVICE_NAME",
                        DEFAULT_OTEL_SERVICE_NAME,
                    ),
                    "OPENCORD_OTEL_SERVICE_NAME",
                )?,
            },
            log: LogConfig {
                format: parse_log_format(value_or_default(lookup, "OPENCORD_LOG_FORMAT", "text"))?,
                env_filter: non_empty_value(
                    value_or_default(lookup, "OPENCORD_LOG_FILTER", DEFAULT_LOG_FILTER),
                    "OPENCORD_LOG_FILTER",
                )?,
            },
            metrics: MetricsConfig {
                prometheus_enabled: parse_bool(value_or_default(
                    lookup,
                    "OPENCORD_METRICS_PROMETHEUS_ENABLED",
                    "true",
                ))?,
            },
        })
    }
}

pub fn api_bind_addr() -> String {
    RuntimeConfig::from_env().bind.api
}

pub fn realtime_bind_addr() -> String {
    RuntimeConfig::from_env().bind.realtime
}

pub fn worker_bind_addr() -> String {
    RuntimeConfig::from_env().bind.worker
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
        DEFAULT_PUBLIC_URL.to_owned()
    } else {
        trimmed
    }
}

fn value_or_default(lookup: &EnvLookup<'_>, key: &str, default: &str) -> String {
    lookup(key).unwrap_or_else(|| default.to_owned())
}

fn non_empty_value(value: String, key: &str) -> Result<String, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(ConfigError::new(format!("{key} must not be blank")))
    } else {
        Ok(trimmed.to_owned())
    }
}

fn parse_endpoint_list(value: String, key: &str) -> Result<Vec<String>, ConfigError> {
    let endpoints: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if endpoints.is_empty() {
        return Err(ConfigError::new(format!(
            "{key} must include at least one host:port endpoint",
        )));
    }

    for endpoint in &endpoints {
        validate_host_port(endpoint, key)?;
    }

    Ok(endpoints)
}

fn validate_host_port(endpoint: &str, key: &str) -> Result<(), ConfigError> {
    if endpoint.contains("://") {
        return Err(ConfigError::new(format!(
            "{key} endpoints must use host:port, got {endpoint}",
        )));
    }

    let Some((host, port)) = endpoint.rsplit_once(':') else {
        return Err(ConfigError::new(format!(
            "{key} endpoint must include a port: {endpoint}",
        )));
    };

    if host.trim().is_empty() || port.parse::<u16>().is_err() {
        return Err(ConfigError::new(format!(
            "{key} endpoint must be host:port, got {endpoint}",
        )));
    }

    Ok(())
}

fn parse_bool(value: String) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::new(format!("invalid boolean value: {other}"))),
    }
}

fn parse_log_format(value: String) -> Result<LogFormat, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "text" | "pretty" | "plain" => Ok(LogFormat::Text),
        "json" => Ok(LogFormat::Json),
        other => Err(ConfigError::new(format!(
            "OPENCORD_LOG_FORMAT must be text or json, got {other}",
        ))),
    }
}
