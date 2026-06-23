use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::domain::auth::AuthError;
use crate::domain::billing::BillingError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::billing::{
    BillingProviderEventRequest, BillingStateResourceResponse, BillingStateResponse,
};
use crate::state::AppState;

pub async fn apply_provider_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BillingProviderEventRequest>,
) -> Result<Json<BillingStateResourceResponse>, BillingApiError> {
    let token = bearer_token(&headers)?;
    state.auth.user_for_token(token).await?;
    let billing = state.billing.apply_provider_event(request.into()).await?;

    Ok(Json(BillingStateResourceResponse {
        billing: BillingStateResponse::from(billing),
    }))
}

#[derive(Debug)]
pub enum BillingApiError {
    Auth(AuthError),
    Billing(BillingError),
}

impl From<AuthError> for BillingApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<BillingError> for BillingApiError {
    fn from(error: BillingError) -> Self {
        Self::Billing(error)
    }
}

impl IntoResponse for BillingApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Billing(error) => (error.status_code(), error.code(), error.message()),
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorDetail { code, message },
            }),
        )
            .into_response()
    }
}
