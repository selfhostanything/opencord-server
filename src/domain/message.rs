use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Option<Uuid>,
    pub channel_id: Uuid,
    pub author_user_id: Uuid,
    pub content: String,
    pub content_format: String,
    pub embeds: Vec<Value>,
    pub components: Vec<Value>,
    pub webhook_username: Option<String>,
    pub webhook_avatar_url: Option<String>,
    pub mention_user_ids: Vec<Uuid>,
    pub mention_role_ids: Vec<Uuid>,
    pub mention_everyone: bool,
    pub reply_to_message_id: Option<Uuid>,
    pub edited_at: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
}

pub struct CreateMessageInput {
    pub organization_id: Uuid,
    pub space_id: Option<Uuid>,
    pub channel_id: Uuid,
    pub author_user_id: Uuid,
    pub content: String,
    pub allow_empty_content: bool,
    pub embeds: Vec<Value>,
    pub components: Vec<Value>,
    pub webhook_username: Option<String>,
    pub webhook_avatar_url: Option<String>,
    pub mention_user_ids: Vec<Uuid>,
    pub mention_role_ids: Vec<Uuid>,
    pub mention_everyone: bool,
    pub reply_to_message_id: Option<Uuid>,
}

#[derive(Debug)]
pub enum MessageError {
    InvalidInput(&'static str),
    NotFound,
    StoreUnavailable,
}

impl MessageError {
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
            Self::NotFound => "message_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::NotFound => "message was not found",
            Self::StoreUnavailable => "message store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait MessageStore: Send + Sync {
    async fn create_message(&self, message: Message) -> Result<(), MessageError>;
    async fn list_for_channel(&self, channel_id: Uuid) -> Result<Vec<Message>, MessageError>;
    async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<Message>, MessageError>;
    async fn get_message(&self, message_id: Uuid) -> Result<Option<Message>, MessageError>;
    async fn update_message(&self, message: Message) -> Result<Message, MessageError>;
    async fn delete_message(&self, message: Message) -> Result<(), MessageError>;
    async fn purge_for_retention(
        &self,
        organization_id: Uuid,
        created_before: Option<String>,
        deleted_before: Option<String>,
        dry_run: bool,
    ) -> Result<usize, MessageError>;
}

#[derive(Clone)]
pub struct MessageService {
    store: std::sync::Arc<dyn MessageStore>,
}

impl MessageService {
    pub fn new(store: std::sync::Arc<dyn MessageStore>) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        organization_id: Uuid,
        space_id: Option<Uuid>,
        channel_id: Uuid,
        author_user_id: Uuid,
        content: String,
        allow_empty_content: bool,
    ) -> Result<Message, MessageError> {
        self.create_with_embeds(CreateMessageInput {
            organization_id,
            space_id,
            channel_id,
            author_user_id,
            content,
            allow_empty_content,
            embeds: Vec::new(),
            components: Vec::new(),
            webhook_username: None,
            webhook_avatar_url: None,
            mention_user_ids: Vec::new(),
            mention_role_ids: Vec::new(),
            mention_everyone: false,
            reply_to_message_id: None,
        })
        .await
    }

    pub async fn create_with_embeds(
        &self,
        input: CreateMessageInput,
    ) -> Result<Message, MessageError> {
        let CreateMessageInput {
            organization_id,
            space_id,
            channel_id,
            author_user_id,
            content,
            allow_empty_content,
            embeds,
            components,
            webhook_username,
            webhook_avatar_url,
            mention_user_ids,
            mention_role_ids,
            mention_everyone,
            reply_to_message_id,
        } = input;
        let embeds = normalize_embeds(embeds)?;
        let components = normalize_components(components)?;
        let message = Message {
            id: ids::new_uuid_v7(),
            organization_id,
            space_id,
            channel_id,
            author_user_id,
            content: normalize_content(content, allow_empty_content)?,
            content_format: "plain".to_owned(),
            embeds,
            components,
            webhook_username: normalize_webhook_username(webhook_username)?,
            webhook_avatar_url: normalize_webhook_avatar_url(webhook_avatar_url)?,
            mention_user_ids: normalize_mention_ids(mention_user_ids),
            mention_role_ids: normalize_mention_ids(mention_role_ids),
            mention_everyone,
            reply_to_message_id,
            edited_at: None,
            deleted_at: None,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };

        self.store.create_message(message.clone()).await?;

        Ok(message)
    }

    pub async fn list_for_channel(&self, channel_id: Uuid) -> Result<Vec<Message>, MessageError> {
        self.store.list_for_channel(channel_id).await
    }

    pub async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<Message>, MessageError> {
        self.store
            .list_for_organization_between(organization_id, from, to)
            .await
    }

    pub async fn get(&self, message_id: Uuid) -> Result<Message, MessageError> {
        self.store
            .get_message(message_id)
            .await?
            .ok_or(MessageError::NotFound)
    }

    pub async fn update(&self, message: Message, content: String) -> Result<Message, MessageError> {
        self.update_with_mentions(message, content, Vec::new(), Vec::new(), false, None)
            .await
    }

    pub async fn update_with_mentions(
        &self,
        mut message: Message,
        content: String,
        mention_user_ids: Vec<Uuid>,
        mention_role_ids: Vec<Uuid>,
        mention_everyone: bool,
        components: Option<Vec<Value>>,
    ) -> Result<Message, MessageError> {
        message.content = normalize_content(content, false)?;
        message.mention_user_ids = normalize_mention_ids(mention_user_ids);
        message.mention_role_ids = normalize_mention_ids(mention_role_ids);
        message.mention_everyone = mention_everyone;
        if let Some(components) = components {
            message.components = normalize_components(components)?;
        }
        message.edited_at = Some("now".to_owned());

        self.store.update_message(message).await
    }

    pub async fn delete(&self, mut message: Message) -> Result<(), MessageError> {
        message.deleted_at = Some(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
        self.store.delete_message(message).await
    }
}

fn normalize_content(content: String, allow_empty: bool) -> Result<String, MessageError> {
    let content = content.trim().to_owned();
    let min_len = if allow_empty { 0 } else { 1 };
    if (min_len..=4000).contains(&content.len()) {
        Ok(content)
    } else {
        Err(MessageError::InvalidInput(
            "message content must be between 1 and 4000 characters unless attachments are present",
        ))
    }
}

fn normalize_embeds(embeds: Vec<Value>) -> Result<Vec<Value>, MessageError> {
    if embeds.len() > 10 {
        return Err(MessageError::InvalidInput(
            "message embeds must contain 10 or fewer embeds",
        ));
    }

    if embeds.iter().any(|embed| !embed.is_object()) {
        return Err(MessageError::InvalidInput("message embeds must be objects"));
    }

    Ok(embeds)
}

fn normalize_components(components: Vec<Value>) -> Result<Vec<Value>, MessageError> {
    if components.len() > 5 {
        return Err(MessageError::InvalidInput(
            "message components must contain 5 or fewer action rows",
        ));
    }

    if components.iter().any(|component| !component.is_object()) {
        return Err(MessageError::InvalidInput(
            "message components must be objects",
        ));
    }

    Ok(components)
}

fn normalize_webhook_username(username: Option<String>) -> Result<Option<String>, MessageError> {
    let Some(username) = username else {
        return Ok(None);
    };
    let username = username.trim().to_owned();
    if username.is_empty() {
        return Ok(None);
    }
    if username.len() > 80 {
        return Err(MessageError::InvalidInput(
            "webhook username must be 80 characters or fewer",
        ));
    }

    Ok(Some(username))
}

fn normalize_webhook_avatar_url(
    avatar_url: Option<String>,
) -> Result<Option<String>, MessageError> {
    let Some(avatar_url) = avatar_url else {
        return Ok(None);
    };
    let avatar_url = avatar_url.trim().to_owned();
    if avatar_url.is_empty() {
        return Ok(None);
    }
    if avatar_url.len() > 2048
        || !(avatar_url.starts_with("https://") || avatar_url.starts_with("http://"))
    {
        return Err(MessageError::InvalidInput(
            "webhook avatar_url must be an HTTP or HTTPS URL up to 2048 characters",
        ));
    }

    Ok(Some(avatar_url))
}

fn normalize_mention_ids(ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}
