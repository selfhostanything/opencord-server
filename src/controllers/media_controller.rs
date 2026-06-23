use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::media::{IssueMediaRoomToken, MediaError, MediaRoomType};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::media::{
    CreateMediaRoomTokenRequest, MediaRoomTokenResourceResponse, MediaRoomTokenResponse,
};
use crate::state::AppState;

pub async fn create_room_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMediaRoomTokenRequest>,
) -> Result<impl IntoResponse, MediaApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let room_type = MediaRoomType::parse(&request.room_type)?;
    let organization_id = parse_uuid(
        &request.organization_id,
        "valid organization_id is required",
    )?;
    let space_id = parse_uuid(&request.space_id, "valid space_id is required")?;
    let channel_id = parse_uuid(&request.channel_id, "valid channel_id is required")?;
    let grants = request.grants();
    grants.validate()?;

    let channel = state.channels.get(channel_id).await?;
    if channel.organization_id != organization_id || channel.space_id != space_id {
        return Err(MediaError::InvalidInput(
            "media token channel must belong to the requested organization and space",
        )
        .into());
    }

    let space = state.spaces.get_for_user(user.id, space_id).await?;
    if space.organization_id != organization_id {
        return Err(MediaError::InvalidInput(
            "media token space must belong to the requested organization",
        )
        .into());
    }

    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ConnectVoice)
        .await?;

    if grants.can_publish_audio {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::Speak)
            .await?;
    }
    if grants.can_publish_video {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::UseVideo)
            .await?;
    }
    if grants.can_publish_screen {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::ShareScreen)
            .await?;
    }

    let media = state.media.issue_room_token(IssueMediaRoomToken {
        room_type,
        organization_id,
        space_id,
        channel_id,
        participant_user_id: user.id,
        grants,
    })?;

    Ok((
        StatusCode::CREATED,
        Json(MediaRoomTokenResourceResponse {
            media: MediaRoomTokenResponse::from(media),
        }),
    ))
}

#[derive(Debug)]
pub enum MediaApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Media(MediaError),
}

impl From<AuthError> for MediaApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<ChannelError> for MediaApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for MediaApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for MediaApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<MediaError> for MediaApiError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl IntoResponse for MediaApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Media(error) => (error.status_code(), error.code(), error.message()),
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

fn parse_uuid(value: &str, message: &'static str) -> Result<Uuid, MediaError> {
    Uuid::parse_str(value).map_err(|_| MediaError::InvalidInput(message))
}
