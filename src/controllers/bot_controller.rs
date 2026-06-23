use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::bot::{BotError, CreateBotApplicationInput};
use crate::domain::organization::OrganizationError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::bot::{BotApplicationCreatedResponse, CreateBotApplicationRequest};
use crate::state::AppState;

pub async fn create_application(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<CreateBotApplicationRequest>,
) -> Result<impl IntoResponse, BotApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let created = state
        .bots
        .create_application(CreateBotApplicationInput {
            organization_id,
            created_by_user_id: user.id,
            name: request.name,
            description: request.description,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(BotApplicationCreatedResponse::from(created)),
    ))
}

#[derive(Debug)]
pub enum BotApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Bot(BotError),
}

impl From<AuthError> for BotApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for BotApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<BotError> for BotApiError {
    fn from(error: BotError) -> Self {
        Self::Bot(error)
    }
}

impl IntoResponse for BotApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Bot(error) => (error.status_code(), error.code(), error.message()),
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
