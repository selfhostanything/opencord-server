use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::domain::auth::AuthError;
use crate::domain::calendar_sync::CalendarSyncError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::calendar::{
    CalendarAccountListResponse, CalendarAccountResourceResponse, CalendarAccountResponse,
    ConnectGoogleCalendarRequest,
};
use crate::state::AppState;

pub async fn connect_google(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConnectGoogleCalendarRequest>,
) -> Result<impl IntoResponse, CalendarApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let account = state
        .calendar_sync
        .connect_google_account(user.id, request.into())
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CalendarAccountResourceResponse {
            account: account.into(),
        }),
    ))
}

pub async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CalendarAccountListResponse>, CalendarApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let accounts = state.calendar_sync.list_accounts(user.id).await?;

    Ok(Json(CalendarAccountListResponse {
        accounts: accounts
            .into_iter()
            .map(CalendarAccountResponse::from)
            .collect(),
    }))
}

#[derive(Debug)]
pub enum CalendarApiError {
    Auth(AuthError),
    Calendar(CalendarSyncError),
}

impl From<AuthError> for CalendarApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<CalendarSyncError> for CalendarApiError {
    fn from(error: CalendarSyncError) -> Self {
        Self::Calendar(error)
    }
}

impl IntoResponse for CalendarApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Calendar(error) => (error.status_code(), error.code(), error.message()),
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
