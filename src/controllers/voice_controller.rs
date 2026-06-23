use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::media::{IssueMediaRoomToken, MediaError, MediaRoomType, MediaTokenGrants};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::media::MediaRoomTokenResponse;
use crate::models::voice::{
    JoinVoiceChannelRequest, VoiceJoinResponse, VoiceMediaEventResponse, VoiceParticipantResponse,
};
use crate::state::AppState;

pub async fn join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<JoinVoiceChannelRequest>,
) -> Result<impl IntoResponse, VoiceApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    if channel.kind != "voice" {
        return Err(MediaError::InvalidInput("channel must be a voice channel").into());
    }

    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ConnectVoice)
        .await?;

    let can_speak = state
        .permissions
        .can_in_channel(user.id, &space, &channel, Permission::Speak)
        .await?;
    let self_mute = request.self_mute();
    let self_deaf = request.self_deaf();
    let grants = MediaTokenGrants {
        can_publish_audio: can_speak && !self_mute,
        can_publish_video: false,
        can_publish_screen: false,
        can_subscribe: true,
    };
    let media = state.media.issue_room_token(IssueMediaRoomToken {
        room_type: MediaRoomType::VoiceChannel,
        organization_id: channel.organization_id,
        space_id: channel.space_id,
        channel_id: channel.id,
        participant_user_id: user.id,
        grants,
    })?;
    let media_response = MediaRoomTokenResponse::from(media);
    let participant = VoiceParticipantResponse {
        channel_id: channel.id.to_string(),
        user_id: user.id.to_string(),
        self_mute,
        self_deaf,
    };
    let event_media = VoiceMediaEventResponse::from(media_response.clone());

    state.realtime.publish(RealtimeEvent::channel(
        "voice.participant_joined",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({
            "participant": participant.clone(),
            "media": event_media
        }),
    ));

    Ok((
        StatusCode::CREATED,
        Json(VoiceJoinResponse {
            voice: participant,
            media: media_response,
        }),
    ))
}

#[derive(Debug)]
pub enum VoiceApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Media(MediaError),
}

impl From<AuthError> for VoiceApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<ChannelError> for VoiceApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for VoiceApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for VoiceApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<MediaError> for VoiceApiError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl IntoResponse for VoiceApiError {
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
