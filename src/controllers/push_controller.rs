use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, extract::State};

use crate::domain::auth::AuthError;
use crate::domain::push::PushError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::push::{
    PushTokenListResponse, PushTokenResourceResponse, PushTokenResponse, RegisterPushTokenRequest,
};
use crate::state::AppState;

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterPushTokenRequest>,
) -> Result<impl IntoResponse, PushApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let push_token = state
        .push
        .register(
            user.id,
            request.platform,
            request.token,
            request.device_name,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(PushTokenResourceResponse {
            push_token: PushTokenResponse::from(push_token),
        }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PushTokenListResponse>, PushApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let push_tokens = state.push.list_for_user(user.id).await?;

    Ok(Json(PushTokenListResponse {
        push_tokens: push_tokens
            .into_iter()
            .map(PushTokenResponse::from)
            .collect(),
    }))
}

#[derive(Debug)]
pub enum PushApiError {
    Auth(AuthError),
    Push(PushError),
}

impl From<AuthError> for PushApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<PushError> for PushApiError {
    fn from(error: PushError) -> Self {
        Self::Push(error)
    }
}

impl IntoResponse for PushApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Push(error) => (error.status_code(), error.code(), error.message()),
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
