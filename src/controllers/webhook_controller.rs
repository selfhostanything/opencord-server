use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::controllers::message_controller::message_response;
use crate::domain::audit::{AuditError, NewAuditEvent};
use crate::domain::auth::AuthError;
use crate::domain::channel::{Channel, ChannelError};
use crate::domain::message::{CreateMessageInput, MessageError};
use crate::domain::organization::OrganizationError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::rate_limit::RateLimitDecision;
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::SpaceError;
use crate::domain::webhook::{IncomingWebhook, WebhookError};
use crate::http::rate_limit::rate_limit_headers;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::message::MessageResourceResponse;
use crate::models::webhook::{
    CreateIncomingWebhookRequest, ExecuteIncomingWebhookRequest, IncomingWebhookDetailResponse,
    IncomingWebhookListResponse, IncomingWebhookResourceResponse, IncomingWebhookResponse,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<CreateIncomingWebhookRequest>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let (user_id, channel) = manageable_text_channel(&state, &headers, channel_id).await?;

    let created = state
        .webhooks
        .create(
            channel.organization_id,
            channel.space_id,
            channel.id,
            user_id,
            request.name,
        )
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: created.webhook.organization_id,
            space_id: created.webhook.space_id,
            actor_user_id: user_id,
            action: "webhook.created",
            target_type: "incoming_webhook",
            target_id: created.webhook.id,
            metadata: json!({
                "channel_id": created.webhook.channel_id,
                "bot_user_id": created.webhook.bot_user_id,
                "name": created.webhook.name,
                "token_last_four": created.webhook.token_last_four
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(IncomingWebhookResourceResponse {
            webhook: webhook_response(created.webhook, created.token, &state.config.public_url),
        }),
    ))
}

pub async fn list_for_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let (_, channel) = manageable_text_channel(&state, &headers, channel_id).await?;
    let webhooks = state.webhooks.list_for_channel(channel.id).await?;

    Ok(Json(IncomingWebhookListResponse {
        webhooks: webhooks
            .into_iter()
            .map(webhook_detail_response)
            .collect::<Vec<_>>(),
    }))
}

pub async fn rotate_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, webhook_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let (user_id, channel) = manageable_text_channel(&state, &headers, channel_id).await?;
    let rotated = state.webhooks.rotate_token(webhook_id, channel.id).await?;
    ensure_webhook_matches_channel(&rotated.webhook, &channel)?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: rotated.webhook.organization_id,
            space_id: rotated.webhook.space_id,
            actor_user_id: user_id,
            action: "webhook.token_rotated",
            target_type: "incoming_webhook",
            target_id: rotated.webhook.id,
            metadata: json!({
                "channel_id": rotated.webhook.channel_id,
                "token_last_four": rotated.webhook.token_last_four
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(IncomingWebhookResourceResponse {
            webhook: webhook_response(rotated.webhook, rotated.token, &state.config.public_url),
        }),
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((channel_id, webhook_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let (user_id, channel) = manageable_text_channel(&state, &headers, channel_id).await?;
    let webhook = state.webhooks.disable(webhook_id, channel.id).await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: webhook.organization_id,
            space_id: webhook.space_id,
            actor_user_id: user_id,
            action: "webhook.deleted",
            target_type: "incoming_webhook",
            target_id: webhook.id,
            metadata: json!({
                "channel_id": webhook.channel_id,
                "bot_user_id": webhook.bot_user_id,
                "name": webhook.name
            }),
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn execute(
    State(state): State<AppState>,
    Path((webhook_id, webhook_token)): Path<(Uuid, String)>,
    Json(request): Json<ExecuteIncomingWebhookRequest>,
) -> Result<impl IntoResponse, WebhookApiError> {
    let rate_limit = state
        .webhook_execution_rate_limits
        .check(public_webhook_bucket(webhook_id));
    if !rate_limit.allowed {
        return Err(WebhookApiError::RateLimited(rate_limit));
    }

    let webhook = state.webhooks.verify(webhook_id, &webhook_token).await?;
    let channel = state.channels.get(webhook.channel_id).await?;
    ensure_webhook_matches_channel(&webhook, &channel)?;
    let policy = state
        .organizations
        .webhook_policy_for_organization(webhook.organization_id)
        .await?;
    let allow_empty_content = !request.embeds.is_empty();
    let (webhook_username, webhook_avatar_url) = if policy.allow_identity_overrides {
        (request.username, request.avatar_url)
    } else {
        (None, None)
    };

    let message = state
        .messages
        .create_with_embeds(CreateMessageInput {
            organization_id: webhook.organization_id,
            space_id: Some(webhook.space_id),
            channel_id: webhook.channel_id,
            author_user_id: webhook.bot_user_id,
            content: request.content.unwrap_or_default(),
            allow_empty_content,
            embeds: request.embeds,
            components: Vec::new(),
            webhook_username,
            webhook_avatar_url,
            mention_user_ids: Vec::new(),
            mention_role_ids: Vec::new(),
            mention_everyone: false,
            reply_to_message_id: None,
        })
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
        rate_limit_headers(&rate_limit),
        Json(MessageResourceResponse { message }),
    ))
}

#[derive(Debug)]
pub enum WebhookApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Organization(OrganizationError),
    Webhook(WebhookError),
    Message(MessageError),
    Audit(AuditError),
    RateLimited(RateLimitDecision),
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

impl From<OrganizationError> for WebhookApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
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

impl From<AuditError> for WebhookApiError {
    fn from(error: AuditError) -> Self {
        Self::Audit(error)
    }
}

impl IntoResponse for WebhookApiError {
    fn into_response(self) -> Response {
        if let Self::RateLimited(decision) = self {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                rate_limit_headers(&decision),
                Json(ErrorResponse {
                    error: ErrorDetail {
                        code: "rate_limited",
                        message: "rate limit exceeded",
                    },
                }),
            )
                .into_response();
        }

        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Webhook(error) => (error.status_code(), error.code(), error.message()),
            Self::Message(error) => (error.status_code(), error.code(), error.message()),
            Self::Audit(error) => (error.status_code(), error.code(), error.message()),
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

fn public_webhook_bucket(webhook_id: Uuid) -> String {
    format!("webhook:{webhook_id}")
}

async fn manageable_text_channel(
    state: &AppState,
    headers: &HeaderMap,
    channel_id: Uuid,
) -> Result<(Uuid, Channel), WebhookApiError> {
    let token = bearer_token(headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ManageChannels)
        .await?;
    ensure_text_channel(&channel)?;

    Ok((user.id, channel))
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

fn webhook_detail_response(webhook: IncomingWebhook) -> IncomingWebhookDetailResponse {
    IncomingWebhookDetailResponse {
        id: webhook.id.to_string(),
        organization_id: webhook.organization_id.to_string(),
        space_id: webhook.space_id.to_string(),
        channel_id: webhook.channel_id.to_string(),
        bot_user_id: webhook.bot_user_id.to_string(),
        created_by_user_id: webhook.created_by_user_id.to_string(),
        name: webhook.name,
        status: webhook.status,
        token_last_four: webhook.token_last_four,
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
