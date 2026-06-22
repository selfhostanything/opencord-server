use axum::{Json, extract::State};

use crate::models::responses::{CapabilitiesResponse, VersionResponse, WellKnownResponse};
use crate::state::AppState;

pub async fn well_known(State(state): State<AppState>) -> Json<WellKnownResponse> {
    Json(WellKnownResponse {
        server: "opencord",
        version: state.config.version,
        api_base_url: format!("{}/api", state.config.public_url),
        realtime_url: realtime_url(&state.config.public_url),
    })
}

pub async fn version(State(state): State<AppState>) -> Json<VersionResponse> {
    Json(VersionResponse {
        version: state.config.version,
    })
}

pub async fn capabilities() -> Json<CapabilitiesResponse> {
    Json(CapabilitiesResponse {
        capabilities: vec![
            "openapi",
            "health",
            "server_discovery",
            "uuidv7",
            "auth",
            "organizations",
        ],
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
