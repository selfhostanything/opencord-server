use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::controllers::message_controller::attachment_response;
use crate::domain::auth::AuthError;
use crate::domain::data_export::DataExportError;
use crate::domain::organization::OrganizationError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::data_export::{DataExportEnvelope, DataExportResponse};
use crate::state::AppState;

pub async fn export_for_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Query(query): Query<DataExportQuery>,
) -> Result<Json<DataExportEnvelope>, DataExportApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let export = state
        .data_exports
        .export_for_organization(organization_id, query.from, query.to)
        .await?;
    let files = export
        .files
        .iter()
        .cloned()
        .map(|file| attachment_response(file, &state.config.public_url))
        .collect();

    Ok(Json(DataExportEnvelope {
        export: DataExportResponse::from_export(export, files),
    }))
}

#[derive(Debug, Deserialize)]
pub struct DataExportQuery {
    pub from: String,
    pub to: String,
}

#[derive(Debug)]
pub enum DataExportApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    DataExport(DataExportError),
}

impl From<AuthError> for DataExportApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for DataExportApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<DataExportError> for DataExportApiError {
    fn from(error: DataExportError) -> Self {
        Self::DataExport(error)
    }
}

impl IntoResponse for DataExportApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::DataExport(error) => (error.status_code(), error.code(), error.message()),
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
