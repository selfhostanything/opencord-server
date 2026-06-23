use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::attachment::{Attachment, AttachmentError};
use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::message::{Message, MessageError};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::attachment::AttachmentResponse;
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

    state
        .attachments
        .validate_for_message(
            channel.organization_id,
            channel.space_id,
            channel.id,
            user.id,
            &request.attachment_ids,
        )
        .await?;

    let attachment_ids = request.attachment_ids;
    let allow_empty_content = !attachment_ids.is_empty();
    let message = state
        .messages
        .create(
            channel.organization_id,
            Some(channel.space_id),
            channel.id,
            user.id,
            request.content,
            allow_empty_content,
        )
        .await?;
    let attachments = state
        .attachments
        .link_to_message(message.id, &attachment_ids)
        .await?;
    let message = message_response(message, attachments, &state.config.public_url);
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
    let message_ids = messages
        .iter()
        .map(|message| message.id)
        .collect::<Vec<_>>();
    let attachments = state.attachments.list_for_message_ids(&message_ids).await?;
    let mut attachments_by_message_id = attachments_by_message_id(attachments);

    Ok(Json(MessageListResponse {
        messages: messages
            .into_iter()
            .map(|message| {
                let attachments = attachments_by_message_id
                    .remove(&message.id)
                    .unwrap_or_default();
                message_response(message, attachments, &state.config.public_url)
            })
            .collect(),
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
    let attachments = state
        .attachments
        .list_for_message_ids(&[message.id])
        .await?;
    let message = message_response(message, attachments, &state.config.public_url);
    state.realtime.publish(RealtimeEvent::channel(
        "message.updated",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({ "message": message.clone() }),
    ));

    Ok(Json(MessageResourceResponse { message }))
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

    let message_id = message.id;
    state.messages.delete(message).await?;
    state.realtime.publish(RealtimeEvent::channel(
        "message.deleted",
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({
            "id": message_id.to_string(),
            "channel_id": channel.id.to_string(),
            "guild_id": channel.space_id.to_string()
        }),
    ));

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug)]
pub enum MessageApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Message(MessageError),
    Attachment(AttachmentError),
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

impl From<AttachmentError> for MessageApiError {
    fn from(error: AttachmentError) -> Self {
        Self::Attachment(error)
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
            Self::Attachment(error) => (error.status_code(), error.code(), error.message()),
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

pub(crate) fn message_response(
    message: Message,
    attachments: Vec<Attachment>,
    public_url: &str,
) -> MessageResponse {
    MessageResponse {
        id: message.id.to_string(),
        organization_id: message.organization_id.to_string(),
        space_id: message.space_id.map(|id| id.to_string()),
        channel_id: message.channel_id.to_string(),
        author_user_id: message.author_user_id.to_string(),
        content: message.content,
        content_format: message.content_format,
        embeds: message.embeds,
        components: message.components,
        webhook_username: message.webhook_username,
        webhook_avatar_url: message.webhook_avatar_url,
        reply_to_message_id: message.reply_to_message_id.map(|id| id.to_string()),
        edited_at: message.edited_at,
        deleted_at: message.deleted_at,
        created_at: message.created_at,
        attachments: attachments
            .into_iter()
            .map(|attachment| attachment_response(attachment, public_url))
            .collect(),
    }
}

fn attachments_by_message_id(attachments: Vec<Attachment>) -> HashMap<Uuid, Vec<Attachment>> {
    let mut attachments_by_message_id = HashMap::new();
    for attachment in attachments {
        if let Some(message_id) = attachment.message_id {
            attachments_by_message_id
                .entry(message_id)
                .or_insert_with(Vec::new)
                .push(attachment);
        }
    }
    attachments_by_message_id
}

pub(crate) fn attachment_response(attachment: Attachment, public_url: &str) -> AttachmentResponse {
    AttachmentResponse {
        id: attachment.id.to_string(),
        organization_id: attachment.organization_id.to_string(),
        space_id: attachment.space_id.to_string(),
        channel_id: attachment.channel_id.to_string(),
        message_id: attachment.message_id.map(|id| id.to_string()),
        uploader_user_id: attachment.uploader_user_id.to_string(),
        file_name: attachment.file_name,
        content_type: attachment.content_type,
        size_bytes: attachment.size_bytes,
        status: attachment.status.as_str().to_owned(),
        download_url: attachment_download_url(public_url, attachment.id),
    }
}

pub(crate) fn attachment_download_url(public_url: &str, attachment_id: Uuid) -> String {
    format!("{public_url}/attachments/{attachment_id}/content")
}
