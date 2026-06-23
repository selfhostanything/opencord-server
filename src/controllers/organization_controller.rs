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
    CreateCustomDomainRequest, CreateOrganizationRequest, CustomDomainEnvelope,
    CustomDomainListResponse, CustomDomainResolveResponse, CustomDomainResponse,
    OrganizationListResponse, OrganizationMembershipResponse, OrganizationResponse,
    ProvisionTenantRequest, TenantProvisionResponse, VerifyCustomDomainRequest,
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

pub async fn create_custom_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<CreateCustomDomainRequest>,
) -> Result<impl IntoResponse, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let custom_domain = state
        .organizations
        .create_custom_domain(user.id, organization_id, request.hostname)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CustomDomainEnvelope {
            custom_domain: CustomDomainResponse::from(custom_domain),
        }),
    ))
}

pub async fn list_custom_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<CustomDomainListResponse>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let custom_domains = state
        .organizations
        .list_custom_domains(user.id, organization_id)
        .await?;

    Ok(Json(CustomDomainListResponse {
        custom_domains: custom_domains
            .into_iter()
            .map(CustomDomainResponse::from)
            .collect(),
    }))
}

pub async fn verify_custom_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organization_id, custom_domain_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<VerifyCustomDomainRequest>,
) -> Result<Json<CustomDomainEnvelope>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let custom_domain = state
        .organizations
        .verify_custom_domain(
            user.id,
            organization_id,
            custom_domain_id,
            request.verification_token,
        )
        .await?;

    Ok(Json(CustomDomainEnvelope {
        custom_domain: CustomDomainResponse::from(custom_domain),
    }))
}

pub async fn resolve_custom_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CustomDomainResolveResponse>, OrganizationApiError> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(OrganizationError::InvalidInput("host header is required"))?;
    let tenant = state
        .organizations
        .resolve_custom_domain(host.to_owned())
        .await?;

    Ok(Json(CustomDomainResolveResponse::from(tenant)))
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
