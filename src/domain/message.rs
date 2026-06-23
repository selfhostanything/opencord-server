use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Number, Value};
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

    embeds.into_iter().map(normalize_embed).collect()
}

fn normalize_embed(embed: Value) -> Result<Value, MessageError> {
    let object = embed
        .as_object()
        .ok_or(MessageError::InvalidInput("message embeds must be objects"))?;
    if object.keys().any(|key| !is_supported_embed_key(key)) {
        return Err(MessageError::InvalidInput(
            "message embeds contain unsupported fields",
        ));
    }

    let mut normalized = Map::new();
    let mut total_text = 0usize;

    if let Some(title) = optional_embed_text(
        object,
        "title",
        256,
        "embed title must be 256 characters or fewer",
    )? {
        total_text += title.chars().count();
        normalized.insert("title".to_owned(), Value::String(title));
    }

    if let Some(value) = object.get("type") {
        let Some(embed_type) = value.as_str() else {
            return Err(MessageError::InvalidInput("embed type must be rich"));
        };
        if embed_type.trim() != "rich" {
            return Err(MessageError::InvalidInput("embed type must be rich"));
        }
    }
    normalized.insert("type".to_owned(), Value::String("rich".to_owned()));

    if let Some(description) = optional_embed_text(
        object,
        "description",
        4096,
        "embed description must be 4096 characters or fewer",
    )? {
        total_text += description.chars().count();
        normalized.insert("description".to_owned(), Value::String(description));
    }

    if let Some(url) = optional_embed_url(
        object,
        "url",
        "embed url must be an HTTP or HTTPS URL up to 2048 characters",
    )? {
        normalized.insert("url".to_owned(), Value::String(url));
    }

    if let Some(timestamp) = optional_embed_timestamp(object)? {
        normalized.insert("timestamp".to_owned(), Value::String(timestamp));
    }

    if let Some(color) = optional_embed_color(object)? {
        normalized.insert("color".to_owned(), Value::Number(Number::from(color)));
    }

    if let Some(footer) = optional_embed_footer(object, &mut total_text)? {
        normalized.insert("footer".to_owned(), Value::Object(footer));
    }

    if let Some(image) = optional_embed_media(object, "image", "embed image")? {
        normalized.insert("image".to_owned(), Value::Object(image));
    }

    if let Some(thumbnail) = optional_embed_media(object, "thumbnail", "embed thumbnail")? {
        normalized.insert("thumbnail".to_owned(), Value::Object(thumbnail));
    }

    if let Some(author) = optional_embed_author(object, &mut total_text)? {
        normalized.insert("author".to_owned(), Value::Object(author));
    }

    if let Some(fields) = optional_embed_fields(object, &mut total_text)? {
        normalized.insert("fields".to_owned(), Value::Array(fields));
    }

    if normalized.len() == 1 {
        return Err(MessageError::InvalidInput(
            "message embeds must include at least one supported field",
        ));
    }
    if total_text > 6000 {
        return Err(MessageError::InvalidInput(
            "embed total text must be 6000 characters or fewer",
        ));
    }

    Ok(Value::Object(normalized))
}

fn is_supported_embed_key(key: &str) -> bool {
    matches!(
        key,
        "title"
            | "type"
            | "description"
            | "url"
            | "timestamp"
            | "color"
            | "footer"
            | "image"
            | "thumbnail"
            | "author"
            | "fields"
    )
}

fn optional_embed_text(
    object: &Map<String, Value>,
    key: &str,
    max_chars: usize,
    message: &'static str,
) -> Result<Option<String>, MessageError> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(MessageError::InvalidInput(message));
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_chars {
        return Err(MessageError::InvalidInput(message));
    }

    Ok(Some(value))
}

fn optional_embed_url(
    object: &Map<String, Value>,
    key: &str,
    message: &'static str,
) -> Result<Option<String>, MessageError> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(MessageError::InvalidInput(message));
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if !is_http_url(&value) {
        return Err(MessageError::InvalidInput(message));
    }

    Ok(Some(value))
}

fn optional_embed_timestamp(object: &Map<String, Value>) -> Result<Option<String>, MessageError> {
    let Some(value) = object.get("timestamp") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(MessageError::InvalidInput(
            "embed timestamp must be RFC3339",
        ));
    };
    let value = value.trim();
    let timestamp = DateTime::parse_from_rfc3339(value)
        .map_err(|_| MessageError::InvalidInput("embed timestamp must be RFC3339"))?;

    Ok(Some(
        timestamp
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true),
    ))
}

fn optional_embed_color(object: &Map<String, Value>) -> Result<Option<u64>, MessageError> {
    let Some(value) = object.get("color") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_u64() else {
        return Err(MessageError::InvalidInput(
            "embed color must be an integer between 0 and 16777215",
        ));
    };
    if value > 0xFF_FF_FF {
        return Err(MessageError::InvalidInput(
            "embed color must be an integer between 0 and 16777215",
        ));
    }

    Ok(Some(value))
}

fn optional_embed_footer(
    object: &Map<String, Value>,
    total_text: &mut usize,
) -> Result<Option<Map<String, Value>>, MessageError> {
    let Some(value) = object.get("footer") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let footer = value
        .as_object()
        .ok_or(MessageError::InvalidInput("embed footer must be an object"))?;
    if footer
        .keys()
        .any(|key| !matches!(key.as_str(), "text" | "icon_url"))
    {
        return Err(MessageError::InvalidInput(
            "embed footer contains unsupported fields",
        ));
    }

    let text = optional_embed_text(
        footer,
        "text",
        2048,
        "embed footer text must be 2048 characters or fewer",
    )?
    .ok_or(MessageError::InvalidInput("embed footer text is required"))?;
    let mut normalized = Map::new();
    *total_text += text.chars().count();
    normalized.insert("text".to_owned(), Value::String(text));
    if let Some(icon_url) = optional_embed_url(
        footer,
        "icon_url",
        "embed footer icon_url must be an HTTP or HTTPS URL up to 2048 characters",
    )? {
        normalized.insert("icon_url".to_owned(), Value::String(icon_url));
    }

    Ok(Some(normalized))
}

fn optional_embed_media(
    object: &Map<String, Value>,
    key: &str,
    label: &'static str,
) -> Result<Option<Map<String, Value>>, MessageError> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let media = value
        .as_object()
        .ok_or(MessageError::InvalidInput("embed media must be an object"))?;
    if media.keys().any(|key| key != "url") {
        return Err(MessageError::InvalidInput(
            "embed media contains unsupported fields",
        ));
    }

    let url = optional_embed_url(media, "url", embed_media_url_message(label))?
        .ok_or(MessageError::InvalidInput(embed_media_url_message(label)))?;
    let mut normalized = Map::new();
    normalized.insert("url".to_owned(), Value::String(url));

    Ok(Some(normalized))
}

fn embed_media_url_message(label: &'static str) -> &'static str {
    match label {
        "embed image" => "embed image url must be an HTTP or HTTPS URL up to 2048 characters",
        "embed thumbnail" => {
            "embed thumbnail url must be an HTTP or HTTPS URL up to 2048 characters"
        }
        _ => "embed media url must be an HTTP or HTTPS URL up to 2048 characters",
    }
}

fn optional_embed_author(
    object: &Map<String, Value>,
    total_text: &mut usize,
) -> Result<Option<Map<String, Value>>, MessageError> {
    let Some(value) = object.get("author") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let author = value
        .as_object()
        .ok_or(MessageError::InvalidInput("embed author must be an object"))?;
    if author
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "url" | "icon_url"))
    {
        return Err(MessageError::InvalidInput(
            "embed author contains unsupported fields",
        ));
    }

    let name = optional_embed_text(
        author,
        "name",
        256,
        "embed author name must be 256 characters or fewer",
    )?
    .ok_or(MessageError::InvalidInput("embed author name is required"))?;
    let mut normalized = Map::new();
    *total_text += name.chars().count();
    normalized.insert("name".to_owned(), Value::String(name));
    if let Some(url) = optional_embed_url(
        author,
        "url",
        "embed author url must be an HTTP or HTTPS URL up to 2048 characters",
    )? {
        normalized.insert("url".to_owned(), Value::String(url));
    }
    if let Some(icon_url) = optional_embed_url(
        author,
        "icon_url",
        "embed author icon_url must be an HTTP or HTTPS URL up to 2048 characters",
    )? {
        normalized.insert("icon_url".to_owned(), Value::String(icon_url));
    }

    Ok(Some(normalized))
}

fn optional_embed_fields(
    object: &Map<String, Value>,
    total_text: &mut usize,
) -> Result<Option<Vec<Value>>, MessageError> {
    let Some(value) = object.get("fields") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let fields = value
        .as_array()
        .ok_or(MessageError::InvalidInput("embed fields must be an array"))?;
    if fields.len() > 25 {
        return Err(MessageError::InvalidInput(
            "embed fields must contain 25 or fewer fields",
        ));
    }

    let mut normalized_fields = Vec::with_capacity(fields.len());
    for field in fields {
        let field = field.as_object().ok_or(MessageError::InvalidInput(
            "embed fields must contain objects",
        ))?;
        if field
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "value" | "inline"))
        {
            return Err(MessageError::InvalidInput(
                "embed fields contain unsupported fields",
            ));
        }

        let name = optional_embed_text(
            field,
            "name",
            256,
            "embed field name must be 256 characters or fewer",
        )?
        .ok_or(MessageError::InvalidInput("embed field name is required"))?;
        let value = optional_embed_text(
            field,
            "value",
            1024,
            "embed field value must be 1024 characters or fewer",
        )?
        .ok_or(MessageError::InvalidInput("embed field value is required"))?;
        let inline = match field.get("inline") {
            Some(Value::Bool(inline)) => *inline,
            Some(Value::Null) | None => false,
            Some(_) => {
                return Err(MessageError::InvalidInput(
                    "embed field inline must be a boolean",
                ));
            }
        };
        *total_text += name.chars().count() + value.chars().count();

        let mut normalized_field = Map::new();
        normalized_field.insert("name".to_owned(), Value::String(name));
        normalized_field.insert("value".to_owned(), Value::String(value));
        normalized_field.insert("inline".to_owned(), Value::Bool(inline));
        normalized_fields.push(Value::Object(normalized_field));
    }

    Ok(Some(normalized_fields))
}

fn is_http_url(value: &str) -> bool {
    value.len() <= 2048 && (value.starts_with("https://") || value.starts_with("http://"))
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
