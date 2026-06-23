use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::organization::{OrganizationError, OrganizationMembership};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::organization::{
    CreateOrganizationRequest, OrganizationListResponse, OrganizationMembershipResponse,
    OrganizationResponse, ProvisionTenantRequest, TenantProvisionResponse,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<impl IntoResponse, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let organization = state.organizations.create(user, request.name).await?;

    Ok((
        StatusCode::CREATED,
        Json(OrganizationMembershipResponse::from(organization)),
    ))
}

pub async fn provision_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProvisionTenantRequest>,
) -> Result<impl IntoResponse, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let tenant = state
        .organizations
        .provision_tenant(
            user,
            request.name,
            request.plan,
            request.deployment_mode,
            request.primary_region,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(TenantProvisionResponse::from(tenant)),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrganizationListResponse>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let organizations = state.organizations.list_for_user(user.id).await?;

    Ok(Json(OrganizationListResponse {
        organizations: organizations
            .into_iter()
            .map(OrganizationResponse::from)
            .collect(),
    }))
}

pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<OrganizationMembershipResponse>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let organization = state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;

    Ok(Json(OrganizationMembershipResponse::from(organization)))
}

#[derive(Debug)]
pub enum OrganizationApiError {
    Auth(AuthError),
    Organization(OrganizationError),
}

impl From<AuthError> for OrganizationApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for OrganizationApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl IntoResponse for OrganizationApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
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

impl From<OrganizationMembership> for OrganizationMembershipResponse {
    fn from(membership: OrganizationMembership) -> Self {
        Self {
            organization: OrganizationResponse::from(membership.clone()),
            membership: crate::models::organization::MembershipResponse {
                role: membership.role,
            },
        }
    }
}
