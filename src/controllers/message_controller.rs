use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::message::{Message, MessageError};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::message::{
    CreateMessageRequest, MessageListResponse, MessageResourceResponse, MessageResponse,
    PatchMessageRequest,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateMessageRequest>,
) -> Result<impl IntoResponse, MessageApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::SendMessages)
        .await?;

    let message = state
        .messages
        .create(
            channel.organization_id,
            Some(channel.space_id),
            channel.id,
            user.id,
            request.content,
        )
        .await?;
    let message = MessageResponse::from(message);
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({ "message": message.clone() }),
    ));

    Ok((
        StatusCode::CREATED,
        Json(MessageResourceResponse { message }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<MessageListResponse>, MessageApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;

    let messages = state.messages.list_for_channel(channel_id).await?;

    Ok(Json(MessageListResponse {
        messages: messages.into_iter().map(MessageResponse::from).collect(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
    Json(request): Json<PatchMessageRequest>,
) -> Result<Json<MessageResourceResponse>, MessageApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let message = state.messages.get(message_id).await?;
    let channel = state.channels.get(message.channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;

    if message.author_user_id != user.id {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::ManageMessages)
            .await?;
    } else {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::SendMessages)
            .await?;
    }

    let message = state.messages.update(message, request.content).await?;

    Ok(Json(MessageResourceResponse {
        message: MessageResponse::from(message),
    }))
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
) -> Result<StatusCode, MessageApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let message = state.messages.get(message_id).await?;
    let channel = state.channels.get(message.channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;

    if message.author_user_id != user.id {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::ManageMessages)
            .await?;
    } else {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::SendMessages)
            .await?;
    }

    state.messages.delete(message).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug)]
pub enum MessageApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Message(MessageError),
    Permission(PermissionError),
}

impl From<AuthError> for MessageApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<ChannelError> for MessageApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for MessageApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<MessageError> for MessageApiError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}

impl From<PermissionError> for MessageApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl IntoResponse for MessageApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Message(error) => (error.status_code(), error.code(), error.message()),
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

impl From<Message> for MessageResponse {
    fn from(message: Message) -> Self {
        Self {
            id: message.id.to_string(),
            organization_id: message.organization_id.to_string(),
            space_id: message.space_id.map(|id| id.to_string()),
            channel_id: message.channel_id.to_string(),
            author_user_id: message.author_user_id.to_string(),
            content: message.content,
            content_format: message.content_format,
            edited_at: message.edited_at,
            deleted_at: message.deleted_at,
        }
    }
}
