use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::channel::{Channel, ChannelError};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::channel::{
    ChannelListResponse, ChannelResourceResponse, ChannelResponse, CreateChannelRequest,
    PatchChannelRequest,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    Json(request): Json<CreateChannelRequest>,
) -> Result<impl IntoResponse, ChannelApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &space, Permission::ManageChannels)
        .await?;
    let channel = state
        .channels
        .create(
            space.organization_id,
            space.id,
            request.kind,
            request.name,
            request.topic,
            request.is_private.unwrap_or(false),
        )
        .await?;
    let response = ChannelResponse::from(channel.clone());
    state.realtime.publish(RealtimeEvent::channel(
        "channel.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({ "channel": response.clone() }),
    ));

    Ok((
        StatusCode::CREATED,
        Json(ChannelResourceResponse { channel: response }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<Json<ChannelListResponse>, ChannelApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;

    let channels = state.channels.list_for_space(space_id).await?;
    let mut visible_channels = Vec::new();
    for channel in channels {
        if state
            .permissions
            .can_in_channel(user.id, &space, &channel, Permission::ViewChannel)
            .await?
        {
            visible_channels.push(channel);
        }
    }

    Ok(Json(ChannelListResponse {
        channels: visible_channels
            .into_iter()
            .map(ChannelResponse::from)
            .collect(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<PatchChannelRequest>,
) -> Result<Json<ChannelResourceResponse>, ChannelApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let existing = state.channels.get(channel_id).await?;
    let space = state
        .spaces
        .get_for_user(user.id, existing.space_id)
        .await?;
    state
        .permissions
        .require_channel(user.id, &space, &existing, Permission::ManageChannels)
        .await?;

    let channel = state.channels.update(existing, request.into()).await?;
    let response = ChannelResponse::from(channel.clone());
    state.realtime.publish(RealtimeEvent::channel(
        "channel.updated",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({ "channel": response.clone() }),
    ));

    Ok(Json(ChannelResourceResponse { channel: response }))
}

#[derive(Debug)]
pub enum ChannelApiError {
    Auth(AuthError),
    Space(SpaceError),
    Channel(ChannelError),
    Permission(PermissionError),
}

impl From<AuthError> for ChannelApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<SpaceError> for ChannelApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<ChannelError> for ChannelApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<PermissionError> for ChannelApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl IntoResponse for ChannelApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
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

impl From<Channel> for ChannelResponse {
    fn from(channel: Channel) -> Self {
        Self {
            id: channel.id.to_string(),
            organization_id: channel.organization_id.to_string(),
            space_id: channel.space_id.to_string(),
            kind: channel.kind,
            name: channel.name,
            slug: channel.slug,
            position: channel.position,
            topic: channel.topic,
            is_private: channel.is_private,
        }
    }
}
