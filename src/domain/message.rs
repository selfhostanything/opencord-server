use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use serde_json::Value;
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
            reply_to_message_id,
        } = input;
        let embeds = normalize_embeds(embeds)?;
        let message = Message {
            id: ids::new_uuid_v7(),
            organization_id,
            space_id,
            channel_id,
            author_user_id,
            content: normalize_content(content, allow_empty_content)?,
            content_format: "plain".to_owned(),
            embeds,
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

    pub async fn update(
        &self,
        mut message: Message,
        content: String,
    ) -> Result<Message, MessageError> {
        message.content = normalize_content(content, false)?;
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
