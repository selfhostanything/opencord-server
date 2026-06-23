use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use uuid::Uuid;

use crate::controllers::message_controller::message_response;
use crate::domain::auth::AuthError;
use crate::domain::bot::{AuthenticatedBot, BotApplication, BotError};
use crate::domain::channel::ChannelError;
use crate::domain::command::{
    CommandError, CreateApplicationCommandInput, CreateCommandInteractionInput,
    CreateComponentInteractionInput, INTERACTION_CALLBACK_CHANNEL_MESSAGE,
    INTERACTION_CALLBACK_DEFERRED_CHANNEL_MESSAGE,
};
use crate::domain::message::Message;
use crate::domain::message::MessageError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::rate_limit::{RateLimitDecision, compat_rest_bot_bucket};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::http::rate_limit::rate_limit_headers;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::command::{
    CommandInteractionCreatedResponse, CompatApplicationCommandResponse,
    CreateCommandInteractionRequest, CreateCompatApplicationCommandRequest,
    CreateComponentInteractionRequest, CreateInteractionCallbackRequest,
    CreateInteractionFollowupRequest, PatchInteractionOriginalResponseRequest,
};
use crate::models::compat::{CompatErrorResponse, CompatMessageResponse, CompatUserResponse};
use crate::state::AppState;

pub async fn create_compat_space_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((application_id, space_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<CreateCompatApplicationCommandRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    if bot.application_id != application_id {
        return Err(CommandError::NotFound.into());
    }

    let space = state.spaces.get_for_user(bot.bot_user_id, space_id).await?;
    if space.organization_id != bot.organization_id {
        return Err(CommandError::NotFound.into());
    }

    let command = state
        .commands
        .create_space_command(CreateApplicationCommandInput {
            bot,
            space_id: Some(space_id),
            name: request.name,
            description: request.description,
            kind: request.kind,
            options: request.options,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        rate_limit_headers(&rate_limit),
        Json(CompatApplicationCommandResponse::from(command)),
    ))
}

pub async fn create_compat_global_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(application_id): Path<Uuid>,
    Json(request): Json<CreateCompatApplicationCommandRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let bot = authenticate_bot(&state, &headers).await?;
    let rate_limit = compat_rest_rate_limit(&state, &bot)?;
    if bot.application_id != application_id {
        return Err(CommandError::NotFound.into());
    }

    let command = state
        .commands
        .create_global_command(CreateApplicationCommandInput {
            bot,
            space_id: None,
            name: request.name,
            description: request.description,
            kind: request.kind,
            options: request.options,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        rate_limit_headers(&rate_limit),
        Json(CompatApplicationCommandResponse::from(command)),
    ))
}

pub async fn create_channel_interaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateCommandInteractionRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;
    let command = state.commands.get_command(request.command_id).await?;
    if command.organization_id != channel.organization_id {
        return Err(CommandError::NotFound.into());
    }
    if command
        .space_id
        .is_some_and(|space_id| space_id != channel.space_id)
    {
        return Err(CommandError::NotFound.into());
    }
    let bot_space = state
        .spaces
        .get_for_user(command.created_by_bot_user_id, channel.space_id)
        .await?;
    if bot_space.organization_id != channel.organization_id
        || !state
            .permissions
            .can_in_channel(
                command.created_by_bot_user_id,
                &bot_space,
                &channel,
                Permission::ViewChannel,
            )
            .await?
    {
        return Err(CommandError::NotFound.into());
    }

    let created = state
        .commands
        .create_interaction(CreateCommandInteractionInput {
            command_id: request.command_id,
            organization_id: channel.organization_id,
            space_id: channel.space_id,
            channel_id: channel.id,
            invoking_user_id: user.id,
            options: request.options,
        })
        .await?;
    let response = CommandInteractionCreatedResponse::from(created);
    let interaction_event =
        serde_json::to_value(&response.interaction).unwrap_or_else(|_| serde_json::json!({}));

    state.realtime.publish(RealtimeEvent::channel(
        "interaction.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        serde_json::json!({
            "interaction": interaction_event,
            "command": CompatApplicationCommandResponse::from(command)
        }),
    ));

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn create_component_interaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateComponentInteractionRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;

    let message = state.messages.get(request.message_id).await?;
    if message.channel_id != channel.id
        || message.organization_id != channel.organization_id
        || message.space_id != Some(channel.space_id)
    {
        return Err(CommandError::NotFound.into());
    }
    let component_type = component_type_for_custom_id(&message.components, &request.custom_id)?;
    let application = state
        .bots
        .application_for_bot_user(message.author_user_id, channel.organization_id)
        .await?;

    let created = state
        .commands
        .create_component_interaction(CreateComponentInteractionInput {
            application_id: application.id,
            organization_id: channel.organization_id,
            space_id: channel.space_id,
            channel_id: channel.id,
            message_id: message.id,
            invoking_user_id: user.id,
            custom_id: request.custom_id,
            component_type,
        })
        .await?;
    let response = CommandInteractionCreatedResponse::from(created);
    let interaction_event =
        serde_json::to_value(&response.interaction).unwrap_or_else(|_| serde_json::json!({}));
    let message_event = realtime_message_value(message, &state.config.public_url);

    state.realtime.publish(RealtimeEvent::channel(
        "interaction.created",
        channel.organization_id,
        channel.space_id,
        channel.id,
        serde_json::json!({
            "interaction": interaction_event,
            "message": message_event
        }),
    ));

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn create_interaction_callback(
    State(state): State<AppState>,
    Path((interaction_id, interaction_token)): Path<(Uuid, String)>,
    Json(request): Json<CreateInteractionCallbackRequest>,
) -> Result<StatusCode, CommandApiError> {
    let interaction = state
        .commands
        .interaction_for_callback(interaction_id, &interaction_token)
        .await?;
    if request.kind == INTERACTION_CALLBACK_DEFERRED_CHANNEL_MESSAGE {
        state
            .commands
            .mark_interaction_deferred(interaction.id)
            .await?;
        return Ok(StatusCode::NO_CONTENT);
    }
    if request.kind != INTERACTION_CALLBACK_CHANNEL_MESSAGE {
        return Err(CommandError::InvalidInput(
            "only channel message and deferred interaction callbacks are supported",
        )
        .into());
    }
    let data = request.data.ok_or(CommandError::InvalidInput(
        "interaction callback data is required",
    ))?;
    let application = state
        .bots
        .application_for_organization(interaction.application_id, interaction.organization_id)
        .await?;
    let channel = state.channels.get(interaction.channel_id).await?;
    if channel.organization_id != interaction.organization_id
        || channel.space_id != interaction.space_id
    {
        return Err(CommandError::NotFound.into());
    }

    let message = state
        .messages
        .create(
            interaction.organization_id,
            Some(interaction.space_id),
            interaction.channel_id,
            application.bot_user_id,
            data.content,
            false,
        )
        .await?;
    let response_message_id = message.id;
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        interaction.organization_id,
        interaction.space_id,
        interaction.channel_id,
        serde_json::json!({
            "message": message_response(message, Vec::new(), &state.config.public_url)
        }),
    ));
    state
        .commands
        .mark_interaction_responded(interaction.id, Some(response_message_id))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_interaction_followup(
    State(state): State<AppState>,
    Path((application_id, interaction_token)): Path<(Uuid, String)>,
    Json(request): Json<CreateInteractionFollowupRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let interaction = state
        .commands
        .interaction_for_followup(application_id, &interaction_token)
        .await?;
    let application = state
        .bots
        .application_for_organization(interaction.application_id, interaction.organization_id)
        .await?;
    let channel = state.channels.get(interaction.channel_id).await?;
    if channel.organization_id != interaction.organization_id
        || channel.space_id != interaction.space_id
    {
        return Err(CommandError::NotFound.into());
    }

    let message = state
        .messages
        .create(
            interaction.organization_id,
            Some(interaction.space_id),
            interaction.channel_id,
            application.bot_user_id,
            request.content,
            false,
        )
        .await?;
    let response_message_id = message.id;
    let response = interaction_followup_response(message.clone(), &application);
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        interaction.organization_id,
        interaction.space_id,
        interaction.channel_id,
        serde_json::json!({
            "message": message_response(message, Vec::new(), &state.config.public_url)
        }),
    ));
    state
        .commands
        .mark_interaction_responded(interaction.id, Some(response_message_id))
        .await?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn update_original_interaction_response(
    State(state): State<AppState>,
    Path((application_id, interaction_token)): Path<(Uuid, String)>,
    Json(request): Json<PatchInteractionOriginalResponseRequest>,
) -> Result<impl IntoResponse, CommandApiError> {
    let interaction = state
        .commands
        .interaction_for_original_response(application_id, &interaction_token)
        .await?;
    let application = state
        .bots
        .application_for_organization(interaction.application_id, interaction.organization_id)
        .await?;
    let response_message_id = interaction
        .response_message_id
        .ok_or(CommandError::NotFound)?;
    let message = state.messages.get(response_message_id).await?;
    if message.organization_id != interaction.organization_id
        || message.space_id != Some(interaction.space_id)
        || message.channel_id != interaction.channel_id
        || message.author_user_id != application.bot_user_id
    {
        return Err(CommandError::NotFound.into());
    }

    let message = state.messages.update(message, request.content).await?;
    let response = interaction_followup_response(message.clone(), &application);
    state.realtime.publish(RealtimeEvent::channel(
        "message.updated",
        interaction.organization_id,
        interaction.space_id,
        interaction.channel_id,
        serde_json::json!({
            "message": message_response(message, Vec::new(), &state.config.public_url)
        }),
    ));
    Ok((StatusCode::OK, Json(response)))
}

pub async fn delete_original_interaction_response(
    State(state): State<AppState>,
    Path((application_id, interaction_token)): Path<(Uuid, String)>,
) -> Result<StatusCode, CommandApiError> {
    let interaction = state
        .commands
        .interaction_for_original_response(application_id, &interaction_token)
        .await?;
    let application = state
        .bots
        .application_for_organization(interaction.application_id, interaction.organization_id)
        .await?;
    let response_message_id = interaction
        .response_message_id
        .ok_or(CommandError::NotFound)?;
    let message = state.messages.get(response_message_id).await?;
    if message.organization_id != interaction.organization_id
        || message.space_id != Some(interaction.space_id)
        || message.channel_id != interaction.channel_id
        || message.author_user_id != application.bot_user_id
    {
        return Err(CommandError::NotFound.into());
    }

    state.messages.delete(message).await?;
    state.realtime.publish(RealtimeEvent::channel(
        "message.deleted",
        interaction.organization_id,
        interaction.space_id,
        interaction.channel_id,
        serde_json::json!({
            "id": response_message_id.to_string(),
            "channel_id": interaction.channel_id.to_string(),
            "guild_id": interaction.space_id.to_string()
        }),
    ));

    Ok(StatusCode::NO_CONTENT)
}

fn interaction_followup_response(
    message: Message,
    application: &BotApplication,
) -> CompatMessageResponse {
    CompatMessageResponse {
        id: message.id.to_string(),
        channel_id: message.channel_id.to_string(),
        author: CompatUserResponse {
            id: application.bot_user_id.to_string(),
            username: application.name.clone(),
            bot: true,
        },
        content: message.content,
        timestamp: message.created_at,
        edited_timestamp: message.edited_at,
        tts: false,
        mention_everyone: false,
        mentions: Vec::new(),
        mention_roles: Vec::new(),
        attachments: Vec::new(),
        embeds: message.embeds,
        components: message.components,
        message_reference: None,
        referenced_message: None,
        pinned: false,
        kind: 0,
    }
}

fn component_type_for_custom_id(
    components: &[Value],
    expected_custom_id: &str,
) -> Result<i32, CommandError> {
    for component in components {
        if let Some(component_type) = matching_component_type(component, expected_custom_id) {
            return Ok(component_type);
        }

        if let Some(children) = component.get("components").and_then(Value::as_array) {
            for child in children {
                if let Some(component_type) = matching_component_type(child, expected_custom_id) {
                    return Ok(component_type);
                }
            }
        }
    }

    Err(CommandError::InvalidInput(
        "component custom_id was not found on the message",
    ))
}

fn matching_component_type(component: &Value, expected_custom_id: &str) -> Option<i32> {
    let custom_id = component.get("custom_id")?.as_str()?;
    if custom_id != expected_custom_id {
        return None;
    }

    component
        .get("type")?
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
}

fn realtime_message_value(
    message: crate::domain::message::Message,
    public_url: &str,
) -> serde_json::Value {
    let embeds = message.embeds.clone();
    let components = message.components.clone();
    let mut value = serde_json::to_value(message_response(message, Vec::new(), public_url))
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("components".to_owned(), Value::Array(components));
        object.insert("embeds".to_owned(), Value::Array(embeds));
        object.insert("mention_everyone".to_owned(), Value::Bool(false));
        object.insert("mentions".to_owned(), Value::Array(Vec::new()));
        object.insert("mention_roles".to_owned(), Value::Array(Vec::new()));
    }
    value
}

#[derive(Debug)]
pub enum CommandApiError {
    Auth(AuthError),
    Bot(BotError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Command(CommandError),
    Message(MessageError),
    RateLimited(RateLimitDecision),
}

impl From<AuthError> for CommandApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<BotError> for CommandApiError {
    fn from(error: BotError) -> Self {
        Self::Bot(error)
    }
}

impl From<ChannelError> for CommandApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for CommandApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for CommandApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<CommandError> for CommandApiError {
    fn from(error: CommandError) -> Self {
        Self::Command(error)
    }
}

impl From<MessageError> for CommandApiError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}

impl IntoResponse for CommandApiError {
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

        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Bot(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Command(error) => (error.status_code(), error.code(), error.message()),
            Self::Message(error) => (error.status_code(), error.code(), error.message()),
            Self::RateLimited(_) => unreachable!("rate limited responses are returned above"),
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

async fn authenticate_bot(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedBot, CommandApiError> {
    let token = bot_token(headers)?;
    Ok(state.bots.authenticate_token(token).await?)
}

fn compat_rest_rate_limit(
    state: &AppState,
    bot: &AuthenticatedBot,
) -> Result<RateLimitDecision, CommandApiError> {
    let decision = state
        .compat_rest_rate_limits
        .check(compat_rest_bot_bucket(bot.application_id));
    if decision.allowed {
        Ok(decision)
    } else {
        Err(CommandApiError::RateLimited(decision))
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

fn bearer_token(headers: &HeaderMap) -> Result<&str, AuthError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::Unauthorized)?;

    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(AuthError::Unauthorized)
}
