use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthStore, StoredUser};
use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncomingWebhook {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub bot_user_id: Uuid,
    pub created_by_user_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_last_four: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncomingWebhookCreated {
    pub webhook: IncomingWebhook,
    pub token: String,
}

#[derive(Debug)]
pub enum WebhookError {
    InvalidInput(&'static str),
    NotFound,
    StoreUnavailable,
}

impl WebhookError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::NotFound => "webhook_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::NotFound => "webhook was not found",
            Self::StoreUnavailable => "webhook store is unavailable",
        }
    }
}

impl From<AuthError> for WebhookError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidInput(message) => Self::InvalidInput(message),
            AuthError::StoreUnavailable => Self::StoreUnavailable,
            AuthError::EmailAlreadyRegistered
            | AuthError::InvalidCredentials
            | AuthError::Unauthorized
            | AuthError::SsoRequired => Self::StoreUnavailable,
        }
    }
}

#[async_trait::async_trait]
pub trait IncomingWebhookStore: Send + Sync {
    async fn create_webhook(&self, webhook: IncomingWebhook) -> Result<(), WebhookError>;
    async fn get_webhook(&self, webhook_id: Uuid) -> Result<Option<IncomingWebhook>, WebhookError>;
    async fn list_webhooks_for_channel(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<IncomingWebhook>, WebhookError>;
    async fn rotate_webhook_token(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
        token_hash: String,
        token_last_four: String,
    ) -> Result<Option<IncomingWebhook>, WebhookError>;
    async fn disable_webhook(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
    ) -> Result<bool, WebhookError>;
}

#[derive(Clone)]
pub struct IncomingWebhookService {
    store: std::sync::Arc<dyn IncomingWebhookStore>,
    auth: std::sync::Arc<dyn AuthStore>,
}

impl IncomingWebhookService {
    pub fn new(
        store: std::sync::Arc<dyn IncomingWebhookStore>,
        auth: std::sync::Arc<dyn AuthStore>,
    ) -> Self {
        Self { store, auth }
    }

    pub async fn create(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        channel_id: Uuid,
        created_by_user_id: Uuid,
        name: String,
    ) -> Result<IncomingWebhookCreated, WebhookError> {
        let id = ids::new_uuid_v7();
        let bot_user_id = ids::new_uuid_v7();
        let name = normalize_name(name)?;
        let token = generate_token();
        let token_last_four = token[token.len() - 4..].to_owned();

        self.auth
            .create_user(StoredUser {
                id: bot_user_id,
                email: format!("webhook-{}@webhooks.opencord.local", id.simple()),
                display_name: name.clone(),
                password_hash: format!("incoming-webhook:{id}"),
            })
            .await?;

        let webhook = IncomingWebhook {
            id,
            organization_id,
            space_id,
            channel_id,
            bot_user_id,
            created_by_user_id,
            name,
            token_hash: hash_token(&token),
            token_last_four,
            status: "active".to_owned(),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };

        self.store.create_webhook(webhook.clone()).await?;

        Ok(IncomingWebhookCreated { webhook, token })
    }

    pub async fn list_for_channel(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<IncomingWebhook>, WebhookError> {
        self.store.list_webhooks_for_channel(channel_id).await
    }

    pub async fn rotate_token(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
    ) -> Result<IncomingWebhookCreated, WebhookError> {
        let token = generate_token();
        let token_last_four = token[token.len() - 4..].to_owned();
        let webhook = self
            .store
            .rotate_webhook_token(webhook_id, channel_id, hash_token(&token), token_last_four)
            .await?
            .ok_or(WebhookError::NotFound)?;

        Ok(IncomingWebhookCreated { webhook, token })
    }

    pub async fn disable(&self, webhook_id: Uuid, channel_id: Uuid) -> Result<(), WebhookError> {
        if self.store.disable_webhook(webhook_id, channel_id).await? {
            Ok(())
        } else {
            Err(WebhookError::NotFound)
        }
    }

    pub async fn verify(
        &self,
        webhook_id: Uuid,
        token: &str,
    ) -> Result<IncomingWebhook, WebhookError> {
        let Some(webhook) = self.store.get_webhook(webhook_id).await? else {
            return Err(WebhookError::NotFound);
        };

        if webhook.status != "active" || webhook.token_hash != hash_token(token) {
            return Err(WebhookError::NotFound);
        }

        Ok(webhook)
    }
}

fn normalize_name(name: String) -> Result<String, WebhookError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=80).contains(&name.len()) {
        Ok(name)
    } else {
        Err(WebhookError::InvalidInput(
            "webhook name must be between 1 and 80 characters",
        ))
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("ocw_{}", hex::encode(bytes))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
