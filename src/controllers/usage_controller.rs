use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::organization::OrganizationError;
use crate::domain::usage::UsageError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::usage::{UsageResourceResponse, UsageSummaryResponse};
use crate::state::AppState;

pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<UsageResourceResponse>, UsageApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;
    let usage = state.usage.summary(organization_id).await?;

    Ok(Json(UsageResourceResponse {
        usage: UsageSummaryResponse::from(usage),
    }))
}

#[derive(Debug)]
pub enum UsageApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Usage(UsageError),
}

impl From<AuthError> for UsageApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for UsageApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<UsageError> for UsageApiError {
    fn from(error: UsageError) -> Self {
        Self::Usage(error)
    }
}

impl IntoResponse for UsageApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Usage(error) => (error.status_code(), error.code(), error.message()),
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
