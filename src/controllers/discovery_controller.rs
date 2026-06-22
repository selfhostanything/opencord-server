use axum::{Json, extract::State};

use crate::config::AppConfig;
use crate::models::responses::{CapabilitiesResponse, VersionResponse, WellKnownResponse};

pub async fn well_known(State(config): State<AppConfig>) -> Json<WellKnownResponse> {
    Json(WellKnownResponse {
        server: "opencord",
        version: config.version,
        api_base_url: format!("{}/api", config.public_url),
        realtime_url: realtime_url(&config.public_url),
    })
}

pub async fn version(State(config): State<AppConfig>) -> Json<VersionResponse> {
    Json(VersionResponse {
        version: config.version,
    })
}

pub async fn capabilities() -> Json<CapabilitiesResponse> {
    Json(CapabilitiesResponse {
        capabilities: vec!["openapi", "health", "server_discovery", "uuidv7"],
    })
}

fn realtime_url(public_url: &str) -> String {
    let websocket_base = if let Some(rest) = public_url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = public_url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        public_url.to_owned()
    };

    format!("{websocket_base}/ws")
}
