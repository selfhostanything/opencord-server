use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::audit::{AuditError, NewAuditEvent};
use crate::domain::auth::AuthError;
use crate::domain::bot::{BotError, CreateBotApplicationInput, RotateBotTokenInput};
use crate::domain::organization::OrganizationError;
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::bot::{
    BotApplicationCreatedResponse, BotApplicationInviteResponse, BotApplicationResponse,
    BotTokenResourceResponse, BotTokenResponse, CreateBotApplicationRequest,
    InviteBotToSpaceRequest,
};
use crate::models::permission::SpaceMemberDetailResponse;
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

pub async fn rotate_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organization_id, application_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, BotApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let token = state
        .bots
        .rotate_token(RotateBotTokenInput {
            organization_id,
            application_id,
            created_by_user_id: user.id,
        })
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id,
            space_id: organization_id,
            actor_user_id: user.id,
            action: "bot.token_rotated",
            target_type: "bot_application",
            target_id: application_id,
            metadata: json!({
                "token_id": token.id,
                "token_last_four": token.token_last_four.clone()
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(BotTokenResourceResponse {
            bot_token: BotTokenResponse::from(token),
        }),
    ))
}

pub async fn invite_to_space(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((organization_id, application_id, space_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(request): Json<InviteBotToSpaceRequest>,
) -> Result<impl IntoResponse, BotApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .require_admin(user.id, organization_id)
        .await?;
    let application = state
        .bots
        .application_for_organization(application_id, organization_id)
        .await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    if space.organization_id != organization_id {
        return Err(BotError::NotFound.into());
    }

    state
        .organizations
        .add_member_if_missing(
            organization_id,
            application.bot_user_id,
            "member".to_owned(),
        )
        .await?;
    let member = state
        .spaces
        .add_member(
            space.id,
            application.bot_user_id,
            request.role.unwrap_or_else(|| "member".to_owned()),
        )
        .await?;
    let role = member.role.clone();
    state
        .audit
        .record(NewAuditEvent {
            organization_id,
            space_id,
            actor_user_id: user.id,
            action: "bot.invited_to_space",
            target_type: "bot_application",
            target_id: application.id,
            metadata: json!({
                "bot_user_id": application.bot_user_id,
                "role": role
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(BotApplicationInviteResponse {
            bot_application: BotApplicationResponse::from(application.clone()),
            member: SpaceMemberDetailResponse {
                space_id: member.id.to_string(),
                user_id: application.bot_user_id.to_string(),
                role: member.role,
                status: "active".to_owned(),
            },
        }),
    ))
}

#[derive(Debug)]
pub enum BotApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Space(SpaceError),
    Audit(AuditError),
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

impl From<SpaceError> for BotApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<AuditError> for BotApiError {
    fn from(error: AuditError) -> Self {
        Self::Audit(error)
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
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Audit(error) => (error.status_code(), error.code(), error.message()),
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
