use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::organization::OrganizationError;
use crate::domain::scim::{ProvisionScimUserInput, ScimError, ScimToken, ScimUser};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::scim::{
    ScimCreateUserRequest, ScimPatchUserRequest, ScimTokenEnvelope, ScimTokenResponse,
    ScimUserResponse,
};
use crate::state::AppState;

pub async fn rotate_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<impl IntoResponse, ScimApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let scim_token = state.scim.rotate_token(organization_id).await?;

    Ok((
        StatusCode::CREATED,
        Json(ScimTokenEnvelope {
            scim_token: ScimTokenResponse::from(scim_token),
        }),
    ))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ScimCreateUserRequest>,
) -> Result<impl IntoResponse, ScimApiError> {
    let token = bearer_token(&headers)?;
    let user = state
        .scim
        .provision_user(
            token,
            ProvisionScimUserInput {
                external_id: request.external_id,
                user_name: request.user_name,
                display_name: request.name.and_then(|name| name.formatted),
                active: request.active,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(ScimUserResponse::from(user))))
}

pub async fn get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(external_id): Path<String>,
) -> Result<Json<ScimUserResponse>, ScimApiError> {
    let token = bearer_token(&headers)?;
    let user = state.scim.get_user(token, external_id).await?;

    Ok(Json(ScimUserResponse::from(user)))
}

pub async fn patch_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(external_id): Path<String>,
    Json(request): Json<ScimPatchUserRequest>,
) -> Result<Json<ScimUserResponse>, ScimApiError> {
    let token = bearer_token(&headers)?;
    let active = active_patch_value(request)?;
    let user = state
        .scim
        .set_user_active(token, external_id, active)
        .await?;

    Ok(Json(ScimUserResponse::from(user)))
}

fn active_patch_value(request: ScimPatchUserRequest) -> Result<bool, ScimError> {
    request
        .operations
        .into_iter()
        .find(|operation| {
            operation.op.eq_ignore_ascii_case("replace")
                && operation
                    .path
                    .as_deref()
                    .is_some_and(|path| path.eq_ignore_ascii_case("active"))
        })
        .and_then(|operation| operation.value.as_bool())
        .ok_or(ScimError::InvalidInput(
            "SCIM patch must replace active with a boolean",
        ))
}

#[derive(Debug)]
pub enum ScimApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Scim(ScimError),
}

impl From<AuthError> for ScimApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for ScimApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<ScimError> for ScimApiError {
    fn from(error: ScimError) -> Self {
        Self::Scim(error)
    }
}

impl IntoResponse for ScimApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Scim(error) => (error.status_code(), error.code(), error.message()),
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

impl From<ScimToken> for ScimTokenResponse {
    fn from(token: ScimToken) -> Self {
        Self {
            organization_id: token.organization_id.to_string(),
            token: token.token,
        }
    }
}

impl From<ScimUser> for ScimUserResponse {
    fn from(user: ScimUser) -> Self {
        Self {
            schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User"],
            id: user.id.to_string(),
            external_id: user.external_id,
            user_name: user.user_name,
            name: crate::models::scim::ScimNameResponse {
                formatted: user.display_name,
            },
            active: user.active,
        }
    }
}
