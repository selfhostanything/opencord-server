use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::controllers::message_controller::message_response;
use crate::domain::auth::AuthError;
use crate::domain::bot::{AuthenticatedBot, BotError};
use crate::domain::channel::ChannelError;
use crate::domain::command::{
    CommandError, CreateApplicationCommandInput, CreateCommandInteractionInput,
};
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
    CreateInteractionCallbackRequest,
};
use crate::models::compat::CompatErrorResponse;
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
            space_id,
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
    let command = state
        .commands
        .get_command(created.interaction.command_id)
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

pub async fn create_interaction_callback(
    State(state): State<AppState>,
    Path((interaction_id, interaction_token)): Path<(Uuid, String)>,
    Json(request): Json<CreateInteractionCallbackRequest>,
) -> Result<StatusCode, CommandApiError> {
    if request.kind != 4 {
        return Err(CommandError::InvalidInput(
            "only channel message interaction callbacks are supported",
        )
        .into());
    }
    let data = request.data.ok_or(CommandError::InvalidInput(
        "interaction callback data is required",
    ))?;
    let interaction = state
        .commands
        .interaction_for_callback(interaction_id, &interaction_token)
        .await?;
    let command = state.commands.get_command(interaction.command_id).await?;
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
            command.created_by_bot_user_id,
            data.content,
            false,
        )
        .await?;
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
        .mark_interaction_responded(interaction.id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
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
