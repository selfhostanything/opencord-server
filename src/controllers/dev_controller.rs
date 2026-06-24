use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::state::AppState;

const DEV_RATE_LIMIT_RESET_ENV: &str = "OPENCORD_DEV_RATE_LIMIT_RESET";

#[derive(Serialize)]
pub struct ResetRateLimitsResponse {
    buckets_cleared: usize,
}

pub async fn reset_rate_limits(
    State(state): State<AppState>,
) -> Result<Json<ResetRateLimitsResponse>, DevApiError> {
    if std::env::var(DEV_RATE_LIMIT_RESET_ENV).ok().as_deref() != Some("1") {
        return Err(DevApiError::Disabled);
    }

    Ok(Json(ResetRateLimitsResponse {
        buckets_cleared: state.clear_rate_limits(),
    }))
}

#[derive(Debug)]
pub enum DevApiError {
    Disabled,
}

impl IntoResponse for DevApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Disabled => StatusCode::NOT_FOUND.into_response(),
        }
    }
}
