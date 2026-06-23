use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::controllers::message_controller::{attachment_download_url, message_response};
use crate::domain::attachment::{Attachment, AttachmentError};
use crate::domain::auth::{AuthError, AuthUser};
use crate::domain::bot::{AuthenticatedBot, BotError};
use crate::domain::channel::{Channel, ChannelError};
use crate::domain::message::{CreateMessageInput, Message, MessageError};
use crate::domain::permission::{Permission, PermissionError, Role};
use crate::domain::rate_limit::{RateLimitDecision, compat_rest_bot_bucket};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::{SpaceError, SpaceMembership};
use crate::http::rate_limit::rate_limit_headers;
use crate::models::compat::{
    CompatChannelResponse, CompatErrorResponse, CompatGuildResponse, CompatMessageReferenceRequest,
    CompatMessageReferenceResponse, CompatMessageResponse, CompatRoleResponse, CompatUserResponse,
    CreateCompatMessageRequest, PatchCompatMessageRequest,
};
use crate::state::AppState;

pub async fn get_current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(compat_user_response(&bot)),
    ))
}

pub async fn get_guild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    let space = visible_space_for_bot(&state, &bot, space_id).await?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(compat_guild_response(space)),
    ))
}

pub async fn list_guild_channels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    let space = visible_space_for_bot(&state, &bot, space_id).await?;
    let channels = state.channels.list_for_space(space.id).await?;
    let mut visible_channels = Vec::new();

    for channel in channels {
        if channel.organization_id != bot.organization_id {
            continue;
        }

        if state
            .permissions
            .can_in_channel(bot.bot_user_id, &space, &channel, Permission::ViewChannel)
            .await?
        {
            visible_channels.push(compat_channel_response(channel));
        }
    }

    Ok((rate_limit_headers(&rate_limit), Json(visible_channels)))
}

pub async fn list_guild_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    let space = visible_space_for_bot(&state, &bot, space_id).await?;
    let roles = state.permissions.list_roles_for_space(space.id).await?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(
            roles
                .into_iter()
                .map(compat_role_response)
                .collect::<Vec<_>>(),
        ),
    ))
}

pub async fn create_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateCompatMessageRequest>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    state
        .permissions
        .require_channel(bot.bot_user_id, &space, &channel, Permission::SendMessages)
        .await?;

    let CreateCompatMessageRequest {
        content,
        embeds,
        components,
        allowed_mentions,
        message_reference,
        tts: _,
    } = request;
    let content = content.unwrap_or_default();
    let mention_ids =
        resolve_allowed_mentions(&state, &space, &content, allowed_mentions.as_ref()).await?;
    let referenced_message =
        validate_message_reference(&state, channel.id, message_reference).await?;
    let referenced_attachments =
        attachments_for_message(&state, referenced_message.as_ref()).await?;
    let reply_to_message_id = referenced_message.as_ref().map(|message| message.id);
    let allow_empty_content = !embeds.is_empty() || !components.is_empty();
    let message = state
        .messages
        .create_with_embeds(CreateMessageInput {
            organization_id: channel.organization_id,
            space_id: Some(channel.space_id),
            channel_id: channel.id,
            author_user_id: bot.bot_user_id,
            content,
            allow_empty_content,
            embeds,
            components,
            mention_user_ids: mention_ids.user_ids,
            mention_role_ids: mention_ids.role_ids,
            mention_everyone: mention_ids.everyone,
            reply_to_message_id,
        })
        .await?;
    let mentions = compat_mentions_for_message(&state, &message, &bot).await?;
    let referenced_mentions = match referenced_message.as_ref() {
        Some(message) => compat_mentions_for_message(&state, message, &bot).await?,
        None => CompatResolvedMentions::default(),
    };
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        serde_json::json!({
            "message": realtime_message_payload(
                message.clone(),
                mentions.clone(),
                referenced_message.clone().map(|message| ReferencedCompatMessage {
                    message,
                    attachments: referenced_attachments.clone(),
                    mentions: referenced_mentions.clone(),
                }),
                &state.config.public_url
            )
        }),
    ));

    Ok((
        rate_limit_headers(&rate_limit),
        Json(compat_message_response(
            message,
            Vec::new(),
            mentions,
            referenced_message.map(|message| ReferencedCompatMessage {
                message,
                attachments: referenced_attachments,
                mentions: referenced_mentions,
            }),
            &bot,
            &state.config.public_url,
        )),
    ))
}

pub async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    let (channel, space) = visible_channel_for_bot(&state, &bot, channel_id).await?;
    state
        .permissions
        .require_channel(bot.bot_user_id, &space, &channel, Permission::ViewChannel)
        .await?;

    let messages = state.messages.list_for_channel(channel.id).await?;
    let message_ids = messages
        .iter()
        .map(|message| message.id)
        .collect::<Vec<_>>();
    let referenced_message_ids = messages
        .iter()
        .filter_map(|message| message.reply_to_message_id)
        .collect::<Vec<_>>();
    let referenced_messages = referenced_messages_by_id(&state, &referenced_message_ids).await?;
    let mut attachment_message_ids = message_ids;
    attachment_message_ids.extend(referenced_messages.keys().copied());
    let attachments = state
        .attachments
        .list_for_message_ids(&attachment_message_ids)
        .await?;
    let attachments_by_message_id = attachments_by_message_id(attachments);
    let mut mention_messages = messages.clone();
    mention_messages.extend(referenced_messages.values().cloned());
    let mentions_by_message_id =
        compat_mentions_by_message_id(&state, &mention_messages, &bot).await?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(
            messages
                .into_iter()
                .map(|message| {
                    let attachments = attachments_by_message_id
                        .get(&message.id)
                        .cloned()
                        .unwrap_or_default();
                    let mentions = mentions_by_message_id
                        .get(&message.id)
                        .cloned()
                        .unwrap_or_default();
                    let referenced_message = message
                        .reply_to_message_id
                        .and_then(|message_id| referenced_messages.get(&message_id))
                        .cloned()
                        .map(|referenced_message| ReferencedCompatMessage {
                            attachments: attachments_by_message_id
                                .get(&referenced_message.id)
                                .cloned()
                                .unwrap_or_default(),
                            mentions: mentions_by_message_id
                                .get(&referenced_message.id)
                                .cloned()
                                .unwrap_or_default(),
                            message: referenced_message,
                        });
                    compat_message_response(
                        message,
                        attachments,
                        mentions,
                        referenced_message,
                        &bot,
                        &state.config.public_url,
                    )
                })
                .collect::<Vec<_>>(),
        ),
    ))
}

pub async fn update_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<PatchCompatMessageRequest>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
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

    let PatchCompatMessageRequest {
        content,
        allowed_mentions,
        components,
    } = request;
    let mention_ids =
        resolve_allowed_mentions(&state, &space, &content, allowed_mentions.as_ref()).await?;
    let message = state
        .messages
        .update_with_mentions(
            message,
            content,
            mention_ids.user_ids,
            mention_ids.role_ids,
            mention_ids.everyone,
            components,
        )
        .await?;
    let attachments = state
        .attachments
        .list_for_message_ids(&[message.id])
        .await?;
    let referenced_message = referenced_message_by_id(&state, message.reply_to_message_id).await?;
    let referenced_attachments =
        attachments_for_message(&state, referenced_message.as_ref()).await?;
    let mentions = compat_mentions_for_message(&state, &message, &bot).await?;
    let referenced_mentions = match referenced_message.as_ref() {
        Some(message) => compat_mentions_for_message(&state, message, &bot).await?,
        None => CompatResolvedMentions::default(),
    };

    Ok((
        rate_limit_headers(&rate_limit),
        Json(compat_message_response(
            message,
            attachments,
            mentions,
            referenced_message.map(|message| ReferencedCompatMessage {
                message,
                attachments: referenced_attachments,
                mentions: referenced_mentions,
            }),
            &bot,
            &state.config.public_url,
        )),
    ))
}

pub async fn delete_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, CompatApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
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

    Ok((rate_limit_headers(&rate_limit), StatusCode::NO_CONTENT))
}

#[derive(Debug)]
pub enum CompatApiError {
    Bot(BotError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Message(MessageError),
    Attachment(AttachmentError),
    Auth(AuthError),
    RateLimited(RateLimitDecision),
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

impl From<AttachmentError> for CompatApiError {
    fn from(error: AttachmentError) -> Self {
        Self::Attachment(error)
    }
}

impl From<AuthError> for CompatApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl IntoResponse for CompatApiError {
    fn into_response(self) -> Response {
        if let Self::RateLimited(decision) = self {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                rate_limit_headers(&decision),
                Json(CompatErrorResponse {
                    message: "rate limit exceeded",
                    code: 42900,
                }),
            )
                .into_response();
        }

        let (status, message) = match self {
            Self::Bot(error) => (error.status_code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.message()),
            Self::Space(error) => (error.status_code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.message()),
            Self::Message(error) => (error.status_code(), error.message()),
            Self::Attachment(error) => (error.status_code(), error.message()),
            Self::Auth(error) => (error.status_code(), error.message()),
            Self::RateLimited(_) => unreachable!("rate limited responses are returned above"),
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

fn compat_rest_rate_limit(
    state: &AppState,
    bot: &AuthenticatedBot,
) -> Result<RateLimitDecision, CompatApiError> {
    let decision = state
        .compat_rest_rate_limits
        .check(compat_rest_bot_bucket(bot.application_id));
    if decision.allowed {
        Ok(decision)
    } else {
        Err(CompatApiError::RateLimited(decision))
    }
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

async fn visible_space_for_bot(
    state: &AppState,
    bot: &AuthenticatedBot,
    space_id: Uuid,
) -> Result<SpaceMembership, CompatApiError> {
    let space = state.spaces.get_for_user(bot.bot_user_id, space_id).await?;
    if space.organization_id != bot.organization_id {
        return Err(SpaceError::NotFound.into());
    }

    Ok(space)
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

#[derive(Clone)]
struct ReferencedCompatMessage {
    message: Message,
    attachments: Vec<Attachment>,
    mentions: CompatResolvedMentions,
}

#[derive(Clone, Default)]
struct CompatResolvedMentions {
    users: Vec<CompatUserResponse>,
    role_ids: Vec<String>,
    everyone: bool,
}

fn compat_message_response(
    message: Message,
    attachments: Vec<Attachment>,
    mentions: CompatResolvedMentions,
    referenced_message: Option<ReferencedCompatMessage>,
    current_bot: &AuthenticatedBot,
    public_url: &str,
) -> CompatMessageResponse {
    let author_is_current_bot = message.author_user_id == current_bot.bot_user_id;
    CompatMessageResponse {
        id: message.id.to_string(),
        channel_id: message.channel_id.to_string(),
        author: if author_is_current_bot {
            compat_user_response(current_bot)
        } else {
            CompatUserResponse {
                id: message.author_user_id.to_string(),
                username: "OpenCord User".to_owned(),
                bot: false,
            }
        },
        content: message.content,
        timestamp: message.created_at,
        edited_timestamp: message.edited_at,
        tts: false,
        mention_everyone: mentions.everyone,
        mentions: mentions.users,
        mention_roles: mentions.role_ids,
        attachments: attachments
            .into_iter()
            .map(|attachment| compat_attachment_response(attachment, public_url))
            .collect(),
        embeds: message.embeds,
        components: message.components,
        message_reference: message.reply_to_message_id.map(|reply_to_message_id| {
            CompatMessageReferenceResponse {
                message_id: reply_to_message_id.to_string(),
                channel_id: message.channel_id.to_string(),
                guild_id: message.space_id.map(|space_id| space_id.to_string()),
            }
        }),
        referenced_message: referenced_message.map(|referenced_message| {
            Box::new(compat_message_response(
                referenced_message.message,
                referenced_message.attachments,
                referenced_message.mentions,
                None,
                current_bot,
                public_url,
            ))
        }),
        pinned: false,
        kind: 0,
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

fn compat_attachment_response(attachment: Attachment, public_url: &str) -> serde_json::Value {
    let url = attachment_download_url(public_url, attachment.id);
    serde_json::json!({
        "id": attachment.id.to_string(),
        "filename": attachment.file_name,
        "size": attachment.size_bytes,
        "url": url,
        "proxy_url": url,
        "content_type": attachment.content_type
    })
}

fn realtime_message_payload(
    message: Message,
    mentions: CompatResolvedMentions,
    referenced_message: Option<ReferencedCompatMessage>,
    public_url: &str,
) -> serde_json::Value {
    let mut value = realtime_message_value(message, Vec::new(), mentions, public_url);
    if let Some(object) = value.as_object_mut()
        && let Some(referenced_message) = referenced_message
    {
        object.insert(
            "referenced_message".to_owned(),
            realtime_message_value(
                referenced_message.message,
                referenced_message.attachments,
                referenced_message.mentions,
                public_url,
            ),
        );
    }
    value
}

fn realtime_message_value(
    message: Message,
    attachments: Vec<Attachment>,
    mentions: CompatResolvedMentions,
    public_url: &str,
) -> serde_json::Value {
    let embeds = message.embeds.clone();
    let components = message.components.clone();
    let mut value = serde_json::to_value(message_response(message, attachments, public_url))
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("embeds".to_owned(), serde_json::Value::Array(embeds));
        object.insert(
            "components".to_owned(),
            serde_json::Value::Array(components),
        );
        object.insert(
            "mention_everyone".to_owned(),
            serde_json::Value::Bool(mentions.everyone),
        );
        object.insert(
            "mentions".to_owned(),
            serde_json::to_value(mentions.users)
                .unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
        );
        object.insert(
            "mention_roles".to_owned(),
            serde_json::Value::Array(
                mentions
                    .role_ids
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    value
}

async fn validate_message_reference(
    state: &AppState,
    channel_id: Uuid,
    message_reference: Option<CompatMessageReferenceRequest>,
) -> Result<Option<Message>, CompatApiError> {
    let Some(message_reference) = message_reference else {
        return Ok(None);
    };

    if message_reference
        .channel_id
        .is_some_and(|referenced_channel_id| referenced_channel_id != channel_id)
    {
        return Err(MessageError::NotFound.into());
    }

    Ok(Some(
        message_in_channel(state, message_reference.message_id, channel_id).await?,
    ))
}

#[derive(Default)]
struct CompatMentionIds {
    user_ids: Vec<Uuid>,
    role_ids: Vec<Uuid>,
    everyone: bool,
}

struct AllowedMentionPolicy {
    parse_users: bool,
    parse_roles: bool,
    parse_everyone: bool,
    explicit_user_ids: HashSet<Uuid>,
    explicit_role_ids: HashSet<Uuid>,
}

impl AllowedMentionPolicy {
    fn allows_user(&self, user_id: Uuid) -> bool {
        self.parse_users || self.explicit_user_ids.contains(&user_id)
    }

    fn allows_role(&self, role_id: Uuid) -> bool {
        self.parse_roles || self.explicit_role_ids.contains(&role_id)
    }
}

async fn resolve_allowed_mentions(
    state: &AppState,
    space: &SpaceMembership,
    content: &str,
    allowed_mentions: Option<&serde_json::Value>,
) -> Result<CompatMentionIds, CompatApiError> {
    let policy = allowed_mention_policy(allowed_mentions);
    let mut mention_ids = CompatMentionIds {
        everyone: has_everyone_mention(content) && policy.parse_everyone,
        ..CompatMentionIds::default()
    };

    for user_id in extract_user_mention_ids(content) {
        if !policy.allows_user(user_id) || mention_ids.user_ids.contains(&user_id) {
            continue;
        }

        if state.spaces.get_for_user(user_id, space.id).await.is_ok()
            && state.auth.user_by_id(user_id).await?.is_some()
        {
            mention_ids.user_ids.push(user_id);
        }
    }

    let role_ids_in_space = state
        .permissions
        .list_roles_for_space(space.id)
        .await?
        .into_iter()
        .map(|role| role.id)
        .collect::<HashSet<_>>();
    for role_id in extract_role_mention_ids(content) {
        if policy.allows_role(role_id)
            && role_ids_in_space.contains(&role_id)
            && !mention_ids.role_ids.contains(&role_id)
        {
            mention_ids.role_ids.push(role_id);
        }
    }

    Ok(mention_ids)
}

fn allowed_mention_policy(allowed_mentions: Option<&serde_json::Value>) -> AllowedMentionPolicy {
    let Some(allowed_mentions) = allowed_mentions else {
        return AllowedMentionPolicy {
            parse_users: true,
            parse_roles: true,
            parse_everyone: true,
            explicit_user_ids: HashSet::new(),
            explicit_role_ids: HashSet::new(),
        };
    };

    let parse_values = allowed_mentions
        .get("parse")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<HashSet<_>>();

    AllowedMentionPolicy {
        parse_users: parse_values.contains("users"),
        parse_roles: parse_values.contains("roles"),
        parse_everyone: parse_values.contains("everyone"),
        explicit_user_ids: uuid_set_from_json_array(allowed_mentions.get("users")),
        explicit_role_ids: uuid_set_from_json_array(allowed_mentions.get("roles")),
    }
}

fn uuid_set_from_json_array(value: Option<&serde_json::Value>) -> HashSet<Uuid> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|value| Uuid::parse_str(value).ok())
        .collect()
}

async fn compat_mentions_by_message_id(
    state: &AppState,
    messages: &[Message],
    current_bot: &AuthenticatedBot,
) -> Result<HashMap<Uuid, CompatResolvedMentions>, CompatApiError> {
    let mut mentions_by_message_id = HashMap::new();
    for message in messages {
        mentions_by_message_id.insert(
            message.id,
            compat_mentions_for_message(state, message, current_bot).await?,
        );
    }
    Ok(mentions_by_message_id)
}

async fn compat_mentions_for_message(
    state: &AppState,
    message: &Message,
    current_bot: &AuthenticatedBot,
) -> Result<CompatResolvedMentions, CompatApiError> {
    let mut users = Vec::new();
    for user_id in &message.mention_user_ids {
        let Some(user) = state.auth.user_by_id(*user_id).await? else {
            continue;
        };
        users.push(compat_user_for_auth_user(user, current_bot));
    }

    Ok(CompatResolvedMentions {
        users,
        role_ids: message
            .mention_role_ids
            .iter()
            .map(Uuid::to_string)
            .collect(),
        everyone: message.mention_everyone,
    })
}

fn compat_user_for_auth_user(user: AuthUser, current_bot: &AuthenticatedBot) -> CompatUserResponse {
    if user.id == current_bot.bot_user_id {
        compat_user_response(current_bot)
    } else {
        CompatUserResponse {
            id: user.id.to_string(),
            username: user.display_name,
            bot: false,
        }
    }
}

fn extract_user_mention_ids(content: &str) -> Vec<Uuid> {
    extract_mention_ids(content, MentionKind::User)
}

fn extract_role_mention_ids(content: &str) -> Vec<Uuid> {
    extract_mention_ids(content, MentionKind::Role)
}

enum MentionKind {
    User,
    Role,
}

fn extract_mention_ids(content: &str, kind: MentionKind) -> Vec<Uuid> {
    let mut ids = Vec::new();
    let mut remaining = content;

    while let Some(start) = remaining.find("<@") {
        let mention = &remaining[start + 2..];
        let Some(end) = mention.find('>') else {
            break;
        };

        let token = &mention[..end];
        let parsed = match kind {
            MentionKind::User => token
                .strip_prefix('!')
                .unwrap_or(token)
                .strip_prefix('&')
                .map(|_| None)
                .unwrap_or_else(|| Uuid::parse_str(token.strip_prefix('!').unwrap_or(token)).ok()),
            MentionKind::Role => token
                .strip_prefix('&')
                .and_then(|role_id| Uuid::parse_str(role_id).ok()),
        };
        if let Some(id) = parsed {
            ids.push(id);
        }

        remaining = &mention[end + 1..];
    }

    ids
}

fn has_everyone_mention(content: &str) -> bool {
    content.contains("@everyone") || content.contains("@here")
}

async fn referenced_messages_by_id(
    state: &AppState,
    message_ids: &[Uuid],
) -> Result<HashMap<Uuid, Message>, CompatApiError> {
    let mut referenced_messages = HashMap::new();
    let mut seen = HashSet::new();
    for message_id in message_ids {
        if !seen.insert(*message_id) {
            continue;
        }

        match state.messages.get(*message_id).await {
            Ok(message) => {
                referenced_messages.insert(message.id, message);
            }
            Err(MessageError::NotFound) => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(referenced_messages)
}

async fn referenced_message_by_id(
    state: &AppState,
    message_id: Option<Uuid>,
) -> Result<Option<Message>, CompatApiError> {
    let Some(message_id) = message_id else {
        return Ok(None);
    };

    match state.messages.get(message_id).await {
        Ok(message) => Ok(Some(message)),
        Err(MessageError::NotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn attachments_for_message(
    state: &AppState,
    message: Option<&Message>,
) -> Result<Vec<Attachment>, CompatApiError> {
    let Some(message) = message else {
        return Ok(Vec::new());
    };

    Ok(state
        .attachments
        .list_for_message_ids(&[message.id])
        .await?)
}

fn compat_user_response(bot: &AuthenticatedBot) -> CompatUserResponse {
    CompatUserResponse {
        id: bot.bot_user_id.to_string(),
        username: bot.name.clone(),
        bot: true,
    }
}

fn compat_guild_response(space: SpaceMembership) -> CompatGuildResponse {
    CompatGuildResponse {
        id: space.id.to_string(),
        name: space.name,
        unavailable: false,
    }
}

fn compat_channel_response(channel: Channel) -> CompatChannelResponse {
    CompatChannelResponse {
        id: channel.id.to_string(),
        guild_id: channel.space_id.to_string(),
        name: channel.name,
        kind: compat_channel_kind(&channel.kind),
        position: channel.position,
        topic: channel.topic,
        nsfw: false,
    }
}

fn compat_channel_kind(kind: &str) -> i32 {
    match kind {
        "voice" => 2,
        _ => 0,
    }
}

fn compat_role_response(role: Role) -> CompatRoleResponse {
    CompatRoleResponse {
        id: role.id.to_string(),
        name: role.name,
        color: compat_role_color(role.color.as_deref()),
        hoist: false,
        position: role.position,
        permissions: role.permissions_bitset.to_string(),
        managed: false,
        mentionable: true,
    }
}

fn compat_role_color(color: Option<&str>) -> i32 {
    color
        .and_then(|color| color.strip_prefix('#'))
        .and_then(|color| i32::from_str_radix(color, 16).ok())
        .unwrap_or(0)
}
