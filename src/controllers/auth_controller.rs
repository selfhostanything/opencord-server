use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, extract::State};

use crate::domain::auth::{AuthError, AuthUser};
use crate::http::session::bearer_token;
use crate::models::auth::{
    AuthResponse, ErrorDetail, ErrorResponse, LoginRequest, MeResponse, RegisterRequest,
    SessionResponse, UserResponse,
};
use crate::state::AppState;

pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthApiError> {
    let result = state
        .auth
        .register(request.email, request.display_name, request.password)
        .await?;

    Ok((StatusCode::CREATED, Json(AuthResponse::from(result))))
}

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AuthApiError> {
    let result = state.auth.login(request.email, request.password).await?;

    Ok(Json(AuthResponse::from(result)))
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
pub struct AuthApiError(AuthError);

impl From<AuthError> for AuthApiError {
    fn from(error: AuthError) -> Self {
        Self(error)
    }
}

impl IntoResponse for AuthApiError {
    fn into_response(self) -> Response {
        let status = self.0.status_code();
        let body = ErrorResponse {
            error: ErrorDetail {
                code: self.0.code(),
                message: self.0.message(),
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
