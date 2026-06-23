use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::audit::AuditError;
use crate::domain::auth::AuthError;
use crate::domain::organization::OrganizationError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::audit::{
    AuditEventListResponse, AuditEventResponse, AuditExportEnvelope, AuditExportResponse,
};
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

pub async fn export_for_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Query(query): Query<AuditExportQuery>,
) -> Result<Json<AuditExportEnvelope>, AuditApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let export = state
        .audit
        .export_for_organization(organization_id, query.from, query.to)
        .await?;

    Ok(Json(AuditExportEnvelope {
        export: AuditExportResponse::from(export),
    }))
}

#[derive(Debug, Deserialize)]
pub struct AuditExportQuery {
    pub from: String,
    pub to: String,
}

#[derive(Debug)]
pub enum AuditApiError {
    Auth(AuthError),
    Organization(OrganizationError),
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

impl From<OrganizationError> for AuditApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
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
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
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
