use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::controllers::message_controller::message_response;
use crate::domain::bot::{AuthenticatedBot, BotError};
use crate::domain::channel::{Channel, ChannelError};
use crate::domain::message::{Message, MessageError};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::{SpaceError, SpaceMembership};
use crate::models::compat::{
    CompatErrorResponse, CompatMessageResponse, CompatUserResponse, CreateCompatMessageRequest,
    PatchCompatMessageRequest,
};
use crate::state::AppState;

pub async fn create_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateCompatMessageRequest>,
) -> Result<Json<CompatMessageResponse>, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    state
        .permissions
        .require_channel(bot.bot_user_id, &space, &channel, Permission::SendMessages)
        .await?;

    let message = state
        .messages
        .create(
            channel.organization_id,
            Some(channel.space_id),
            channel.id,
            bot.bot_user_id,
            request.content,
            false,
        )
        .await?;
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        serde_json::json!({
            "message": message_response(message.clone(), Vec::new(), &state.config.public_url)
        }),
    ));

    Ok(Json(compat_message_response(message, &bot)))
}

pub async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<Vec<CompatMessageResponse>>, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    state
        .permissions
        .require_channel(bot.bot_user_id, &space, &channel, Permission::ViewChannel)
        .await?;

    let messages = state.messages.list_for_channel(channel.id).await?;

    Ok(Json(
        messages
            .into_iter()
            .map(|message| compat_message_response(message, &bot))
            .collect(),
    ))
}

pub async fn update_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<PatchCompatMessageRequest>,
) -> Result<Json<CompatMessageResponse>, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    let message = message_in_channel(&state, message_id, channel.id).await?;

    if message.author_user_id != bot.bot_user_id {
        state
            .permissions
            .require_channel(
                bot.bot_user_id,
                &space,
                &channel,
                Permission::ManageMessages,
            )
            .await?;
    } else {
        state
            .permissions
            .require_channel(bot.bot_user_id, &space, &channel, Permission::SendMessages)
            .await?;
    }

    let message = state.messages.update(message, request.content).await?;

    Ok(Json(compat_message_response(message, &bot)))
}

pub async fn delete_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    let message = message_in_channel(&state, message_id, channel.id).await?;

    if message.author_user_id != bot.bot_user_id {
        state
            .permissions
            .require_channel(
                bot.bot_user_id,
                &space,
                &channel,
                Permission::ManageMessages,
            )
            .await?;
    } else {
        state
            .permissions
            .require_channel(bot.bot_user_id, &space, &channel, Permission::SendMessages)
            .await?;
    }

    state.messages.delete(message).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug)]
pub enum CompatApiError {
    Bot(BotError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Message(MessageError),
}

impl From<BotError> for CompatApiError {
    fn from(error: BotError) -> Self {
        Self::Bot(error)
    }
}

impl From<ChannelError> for CompatApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for CompatApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for CompatApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<MessageError> for CompatApiError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}

impl IntoResponse for CompatApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Bot(error) => (error.status_code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.message()),
            Self::Space(error) => (error.status_code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.message()),
            Self::Message(error) => (error.status_code(), error.message()),
        };

        (status, Json(CompatErrorResponse { message, code: 0 })).into_response()
    }
}

async fn authenticate_bot(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedBot, CompatApiError> {
    let token = bot_token(headers)?;
    Ok(state.bots.authenticate_token(token).await?)
}

fn bot_token(headers: &HeaderMap) -> Result<&str, BotError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(BotError::Unauthorized)?;

    value
        .strip_prefix("Bot ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(BotError::Unauthorized)
}

async fn visible_channel_for_bot(
    state: &AppState,
    bot: &AuthenticatedBot,
    channel_id: Uuid,
) -> Result<(Channel, SpaceMembership), CompatApiError> {
    let channel = state.channels.get(channel_id).await?;
    if channel.organization_id != bot.organization_id {
        return Err(ChannelError::NotFound.into());
    }

    let space = state
        .spaces
        .get_for_user(bot.bot_user_id, channel.space_id)
        .await?;

    Ok((channel, space))
}

async fn message_in_channel(
    state: &AppState,
    message_id: Uuid,
    channel_id: Uuid,
) -> Result<Message, CompatApiError> {
    let message = state.messages.get(message_id).await?;
    if message.channel_id == channel_id {
        Ok(message)
    } else {
        Err(MessageError::NotFound.into())
    }
}

fn compat_message_response(
    message: Message,
    current_bot: &AuthenticatedBot,
) -> CompatMessageResponse {
    let author_is_current_bot = message.author_user_id == current_bot.bot_user_id;
    CompatMessageResponse {
        id: message.id.to_string(),
        channel_id: message.channel_id.to_string(),
        author: CompatUserResponse {
            id: message.author_user_id.to_string(),
            username: if author_is_current_bot {
                current_bot.name.clone()
            } else {
                "OpenCord User".to_owned()
            },
            bot: author_is_current_bot,
        },
        content: message.content,
        timestamp: message.created_at,
        edited_timestamp: message.edited_at,
        tts: false,
        mention_everyone: false,
        mentions: Vec::new(),
        mention_roles: Vec::new(),
        attachments: Vec::new(),
        embeds: Vec::new(),
        pinned: false,
        kind: 0,
    }
}
