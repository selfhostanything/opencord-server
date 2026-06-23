use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::controllers::message_controller::message_response;
use crate::domain::auth::AuthError;
use crate::domain::channel::{Channel, ChannelError};
use crate::domain::message::MessageError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::domain::webhook::{IncomingWebhook, WebhookError};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::message::MessageResourceResponse;
use crate::models::webhook::{
    CreateIncomingWebhookRequest, ExecuteIncomingWebhookRequest, IncomingWebhookResourceResponse,
    IncomingWebhookResponse,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateIncomingWebhookRequest>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ManageChannels)
        .await?;
    ensure_text_channel(&channel)?;

    let created = state
        .webhooks
        .create(
            channel.organization_id,
            channel.space_id,
            channel.id,
            user.id,
            request.name,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(IncomingWebhookResourceResponse {
            webhook: webhook_response(created.webhook, created.token, &state.config.public_url),
        }),
    ))
}

pub async fn execute(
    State(state): State<AppState>,
    Path((webhook_id, webhook_token)): Path<(Uuid, String)>,
    Json(request): Json<ExecuteIncomingWebhookRequest>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let webhook = state.webhooks.verify(webhook_id, &webhook_token).await?;
    let channel = state.channels.get(webhook.channel_id).await?;
    ensure_webhook_matches_channel(&webhook, &channel)?;

    let message = state
        .messages
        .create(
            webhook.organization_id,
            Some(webhook.space_id),
            webhook.channel_id,
            webhook.bot_user_id,
            request.content,
            false,
        )
        .await?;
    let message = message_response(message, Vec::new(), &state.config.public_url);
    state.realtime.publish(RealtimeEvent::channel(
        "message.created",
        webhook.organization_id,
        webhook.space_id,
        webhook.channel_id,
        json!({ "message": message.clone() }),
    ));

    Ok((
        StatusCode::CREATED,
        Json(MessageResourceResponse { message }),
    ))
}

#[derive(Debug)]
pub enum WebhookApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Webhook(WebhookError),
    Message(MessageError),
}

impl From<AuthError> for WebhookApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<ChannelError> for WebhookApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for WebhookApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for WebhookApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<WebhookError> for WebhookApiError {
    fn from(error: WebhookError) -> Self {
        Self::Webhook(error)
    }
}

impl From<MessageError> for WebhookApiError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}

impl IntoResponse for WebhookApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Webhook(error) => (error.status_code(), error.code(), error.message()),
            Self::Message(error) => (error.status_code(), error.code(), error.message()),
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

fn webhook_response(
    webhook: IncomingWebhook,
    token: String,
    public_url: &str,
) -> IncomingWebhookResponse {
    IncomingWebhookResponse {
        id: webhook.id.to_string(),
        organization_id: webhook.organization_id.to_string(),
        space_id: webhook.space_id.to_string(),
        channel_id: webhook.channel_id.to_string(),
        bot_user_id: webhook.bot_user_id.to_string(),
        created_by_user_id: webhook.created_by_user_id.to_string(),
        name: webhook.name,
        status: webhook.status,
        token_last_four: webhook.token_last_four,
        execute_url: format!("{public_url}/api/webhooks/{}/{}", webhook.id, token),
        token,
        created_at: webhook.created_at,
    }
}

fn ensure_text_channel(channel: &Channel) -> Result<(), WebhookError> {
    if channel.kind == "text" {
        Ok(())
    } else {
        Err(WebhookError::InvalidInput(
            "incoming webhooks can only be created for text channels",
        ))
    }
}

fn ensure_webhook_matches_channel(
    webhook: &IncomingWebhook,
    channel: &Channel,
) -> Result<(), WebhookError> {
    if channel.kind == "text"
        && channel.organization_id == webhook.organization_id
        && channel.space_id == webhook.space_id
        && channel.id == webhook.channel_id
    {
        Ok(())
    } else {
        Err(WebhookError::NotFound)
    }
}
