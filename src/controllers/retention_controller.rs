use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::organization::OrganizationError;
use crate::domain::retention::{RetentionError, RetentionPolicy};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::retention::{
    RetentionPolicyEnvelope, RetentionPolicyResponse, RetentionRunListResponse,
    RetentionRunResponse, UpsertRetentionPolicyRequest,
};
use crate::state::AppState;

pub async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<RetentionPolicyEnvelope>, RetentionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let policy = state.retention.get_policy(organization_id).await?;

    Ok(Json(RetentionPolicyEnvelope {
        policy: RetentionPolicyResponse::from(policy),
    }))
}

pub async fn upsert_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<UpsertRetentionPolicyRequest>,
) -> Result<Json<RetentionPolicyEnvelope>, RetentionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let policy = state
        .retention
        .upsert_policy(RetentionPolicy {
            organization_id,
            messages_retain_days: request.messages_retain_days,
            files_retain_days: request.files_retain_days,
            audit_logs_retain_days: request.audit_logs_retain_days,
            deleted_message_purge_days: request.deleted_message_purge_days,
        })
        .await?;

    Ok(Json(RetentionPolicyEnvelope {
        policy: RetentionPolicyResponse::from(policy),
    }))
}

pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<RetentionRunListResponse>, RetentionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let runs = state
        .retention
        .list_runs_for_organization(organization_id)
        .await?;

    Ok(Json(RetentionRunListResponse {
        retention_runs: runs.into_iter().map(RetentionRunResponse::from).collect(),
    }))
}

#[derive(Debug)]
pub enum RetentionApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Retention(RetentionError),
}

impl From<AuthError> for RetentionApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for RetentionApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<RetentionError> for RetentionApiError {
    fn from(error: RetentionError) -> Self {
        Self::Retention(error)
    }
}

impl IntoResponse for RetentionApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Retention(error) => (error.status_code(), error.code(), error.message()),
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
