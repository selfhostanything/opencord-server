use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::domain::auth::{AuthError, AuthUser, OidcProvider};
use crate::domain::rate_limit::{RateLimitDecision, auth_login_bucket, auth_register_bucket};
use crate::http::rate_limit::rate_limit_headers;
use crate::http::session::bearer_token;
use crate::models::auth::{
    AuthResponse, ErrorDetail, ErrorResponse, LoginRequest, MeResponse, OidcCallbackRequest,
    OidcProviderResponse, OidcProvidersQuery, OidcProvidersResponse, RegisterRequest,
    SessionResponse, UserResponse,
};
use crate::state::AppState;

pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthApiError> {
    let rate_limit = state
        .auth_register_rate_limits
        .check(auth_register_bucket(&request.email));
    if !rate_limit.allowed {
        return Err(AuthApiError::RateLimited(rate_limit));
    }
    let result = state
        .auth
        .register(request.email, request.display_name, request.password)
        .await?;

    Ok((
        StatusCode::CREATED,
        rate_limit_headers(&rate_limit),
        Json(AuthResponse::from(result)),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthApiError> {
    let rate_limit = state
        .auth_login_rate_limits
        .check(auth_login_bucket(&request.email));
    if !rate_limit.allowed {
        return Err(AuthApiError::RateLimited(rate_limit));
    }
    let result = state.auth.login(request.email, request.password).await?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(AuthResponse::from(result)),
    ))
}

pub async fn oidc_providers(
    State(state): State<AppState>,
    Query(query): Query<OidcProvidersQuery>,
) -> Result<Json<OidcProvidersResponse>, AuthApiError> {
    let providers = state.auth.oidc_providers_for_email(query.email).await?;

    Ok(Json(OidcProvidersResponse {
        providers: providers
            .into_iter()
            .map(OidcProviderResponse::from)
            .collect(),
    }))
}

pub async fn oidc_callback(
    State(state): State<AppState>,
    Json(request): Json<OidcCallbackRequest>,
) -> Result<Json<AuthResponse>, AuthApiError> {
    let result = state
        .auth
        .oidc_login(crate::domain::auth::OidcProviderAssertion {
            issuer: request.issuer,
            subject: request.subject,
            email: request.email,
            display_name: request.display_name,
            email_verified: request.email_verified,
            signature: request.signature,
        })
        .await?;
    state
        .organizations
        .add_member_if_missing(
            result.organization_id,
            result.auth.user.id,
            result.auto_join_role,
        )
        .await
        .map_err(|_| AuthError::StoreUnavailable)?;

    Ok(Json(AuthResponse::from(result.auth)))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AuthApiError> {
    let token = bearer_token(&headers)?;
    state.auth.logout(token).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, AuthApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;

    Ok(Json(MeResponse {
        user: UserResponse::from(user),
    }))
}

#[derive(Debug)]
pub enum AuthApiError {
    Auth(AuthError),
    RateLimited(RateLimitDecision),
}

impl From<AuthError> for AuthApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl IntoResponse for AuthApiError {
    fn into_response(self) -> Response {
        if let Self::RateLimited(decision) = self {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                rate_limit_headers(&decision),
                Json(ErrorResponse {
                    error: ErrorDetail {
                        code: "rate_limited",
                        message: "rate limit exceeded",
                    },
                }),
            )
                .into_response();
        }

        let Self::Auth(error) = self else {
            unreachable!("rate limited responses are returned above")
        };
        let status = error.status_code();
        let body = ErrorResponse {
            error: ErrorDetail {
                code: error.code(),
                message: error.message(),
            },
        };

        (status, Json(body)).into_response()
    }
}

impl From<AuthUser> for UserResponse {
    fn from(user: AuthUser) -> Self {
        Self {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
        }
    }
}

impl From<OidcProvider> for OidcProviderResponse {
    fn from(provider: OidcProvider) -> Self {
        Self {
            organization_id: provider.organization_id.to_string(),
            issuer: provider.issuer,
            authorization_endpoint: provider.authorization_endpoint,
            token_endpoint: provider.token_endpoint,
            jwks_uri: provider.jwks_uri,
            client_id: provider.client_id,
            allowed_domains: provider.allowed_domains,
            require_sso: provider.require_sso,
            auto_join_role: provider.auto_join_role,
        }
    }
}

impl From<crate::domain::auth::AuthResult> for AuthResponse {
    fn from(result: crate::domain::auth::AuthResult) -> Self {
        Self {
            user: UserResponse::from(result.user),
            session: SessionResponse {
                token: result.session_token,
            },
        }
    }
}
