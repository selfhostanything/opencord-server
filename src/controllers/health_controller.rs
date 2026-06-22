use axum::{Json, extract::State};

use crate::models::responses::HealthResponse;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: state.config.version,
    })
}
