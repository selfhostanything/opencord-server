use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::audit::{AuditError, NewAuditEvent};
use crate::domain::auth::AuthError;
use crate::domain::organization::{OrganizationError, OrganizationMembership};
use crate::http::session::bearer_token;
use crate::models::auth::{
    ConfigureOidcProviderRequest, ErrorDetail, ErrorResponse, OidcProviderEnvelope,
    OidcProviderResponse,
};
use crate::models::organization::{
    CreateCustomDomainRequest, CreateOrganizationRequest, CustomDomainEnvelope,
    CustomDomainListResponse, CustomDomainResolveResponse, CustomDomainResponse,
    OrganizationListResponse, OrganizationMembershipResponse, OrganizationResponse,
    ProvisionTenantRequest, TenantProvisionResponse, UpsertWebhookPolicyRequest,
    VerifyCustomDomainRequest, WebhookPolicyEnvelope, WebhookPolicyResponse,
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

pub async fn get_webhook_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<WebhookPolicyEnvelope>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let policy = state
        .organizations
        .webhook_policy(user.id, organization_id)
        .await?;

    Ok(Json(WebhookPolicyEnvelope {
        webhook_policy: WebhookPolicyResponse::from(policy),
    }))
}

pub async fn upsert_webhook_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<UpsertWebhookPolicyRequest>,
) -> Result<Json<WebhookPolicyEnvelope>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let policy = state
        .organizations
        .upsert_webhook_policy(user.id, organization_id, request.allow_identity_overrides)
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id,
            space_id: organization_id,
            actor_user_id: user.id,
            action: "organization.webhook_policy_updated",
            target_type: "organization",
            target_id: organization_id,
            metadata: json!({
                "allow_identity_overrides": policy.allow_identity_overrides
            }),
        })
        .await?;

    Ok(Json(WebhookPolicyEnvelope {
        webhook_policy: WebhookPolicyResponse::from(policy),
    }))
}

pub async fn configure_oidc_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<ConfigureOidcProviderRequest>,
) -> Result<Json<OidcProviderEnvelope>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let provider = state
        .auth
        .configure_oidc_provider(crate::domain::auth::ConfigureOidcProviderInput {
            organization_id,
            issuer: request.issuer,
            authorization_endpoint: request.authorization_endpoint,
            token_endpoint: request.token_endpoint,
            jwks_uri: request.jwks_uri,
            client_id: request.client_id,
            client_secret: request.client_secret,
            allowed_domains: request.allowed_domains,
            require_sso: request.require_sso,
            auto_join_role: request.auto_join_role,
        })
        .await?;

    Ok(Json(OidcProviderEnvelope {
        provider: OidcProviderResponse::from(provider),
    }))
}

pub async fn get_oidc_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<OidcProviderEnvelope>, OrganizationApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let provider = state
        .auth
        .oidc_provider_for_organization(organization_id)
        .await?
        .ok_or(OrganizationError::NotFound)?;

    Ok(Json(OidcProviderEnvelope {
        provider: OidcProviderResponse::from(provider),
    }))
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
    Audit(AuditError),
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

impl From<AuditError> for OrganizationApiError {
    fn from(error: AuditError) -> Self {
        Self::Audit(error)
    }
}

impl IntoResponse for OrganizationApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
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
