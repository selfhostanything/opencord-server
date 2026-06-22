use axum::{Json, extract::State};

use crate::config::AppConfig;
use crate::models::responses::HealthResponse;

pub async fn health(State(config): State<AppConfig>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: config.version,
    })
}
