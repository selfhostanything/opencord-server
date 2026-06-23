use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::audit::AuditError;
use crate::domain::auth::AuthError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::audit::{AuditEventListResponse, AuditEventResponse};
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::state::AppState;

pub async fn list_for_space(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<Json<AuditEventListResponse>, AuditApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &space, Permission::ManageSpace)
        .await?;

    let audit_events = state.audit.list_for_space(space.id).await?;

    Ok(Json(AuditEventListResponse {
        audit_events: audit_events
            .into_iter()
            .map(AuditEventResponse::from)
            .collect(),
    }))
}

#[derive(Debug)]
pub enum AuditApiError {
    Auth(AuthError),
    Space(SpaceError),
    Permission(PermissionError),
    Audit(AuditError),
}

impl From<AuthError> for AuditApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<SpaceError> for AuditApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for AuditApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<AuditError> for AuditApiError {
    fn from(error: AuditError) -> Self {
        Self::Audit(error)
    }
}

impl IntoResponse for AuditApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Audit(error) => (error.status_code(), error.code(), error.message()),
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
