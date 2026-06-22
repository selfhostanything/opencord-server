use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub position: i32,
    pub topic: Option<String>,
    pub is_private: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChannelPatch {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub is_private: Option<bool>,
}

#[derive(Debug)]
pub enum ChannelError {
    InvalidInput(&'static str),
    SlugAlreadyExists,
    NotFound,
    StoreUnavailable,
}

impl ChannelError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::SlugAlreadyExists => StatusCode::CONFLICT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::SlugAlreadyExists => "channel_slug_already_exists",
            Self::NotFound => "channel_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::SlugAlreadyExists => "channel slug already exists in this space",
            Self::NotFound => "channel was not found",
            Self::StoreUnavailable => "channel store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait ChannelStore: Send + Sync {
    async fn create_channel(&self, channel: Channel) -> Result<(), ChannelError>;
    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<Channel>, ChannelError>;
    async fn get_channel(&self, channel_id: Uuid) -> Result<Option<Channel>, ChannelError>;
    async fn update_channel(&self, channel: Channel) -> Result<Channel, ChannelError>;
}

#[derive(Clone)]
pub struct ChannelService {
    store: std::sync::Arc<dyn ChannelStore>,
}

impl ChannelService {
    pub fn new(store: std::sync::Arc<dyn ChannelStore>) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        name: String,
        topic: Option<String>,
        is_private: bool,
    ) -> Result<Channel, ChannelError> {
        let name = normalize_name(name)?;
        let channel = Channel {
            id: ids::new_uuid_v7(),
            organization_id,
            space_id,
            kind: "text".to_owned(),
            slug: slugify(&name)?,
            name,
            position: 0,
            topic: normalize_topic(topic)?,
            is_private,
        };

        self.store.create_channel(channel.clone()).await?;

        Ok(channel)
    }

    pub async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<Channel>, ChannelError> {
        self.store.list_for_space(space_id).await
    }

    pub async fn get(&self, channel_id: Uuid) -> Result<Channel, ChannelError> {
        self.store
            .get_channel(channel_id)
            .await?
            .ok_or(ChannelError::NotFound)
    }

    pub async fn update(
        &self,
        mut existing: Channel,
        patch: ChannelPatch,
    ) -> Result<Channel, ChannelError> {
        if let Some(name) = patch.name {
            let name = normalize_name(name)?;
            existing.slug = slugify(&name)?;
            existing.name = name;
        }

        if let Some(topic) = patch.topic {
            existing.topic = normalize_topic(Some(topic))?;
        }

        if let Some(position) = patch.position {
            if position < 0 {
                return Err(ChannelError::InvalidInput(
                    "channel position must be greater than or equal to 0",
                ));
            }
            existing.position = position;
        }

        if let Some(is_private) = patch.is_private {
            existing.is_private = is_private;
        }

        self.store.update_channel(existing).await
    }
}

fn normalize_name(name: String) -> Result<String, ChannelError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=100).contains(&name.len()) {
        Ok(name)
    } else {
        Err(ChannelError::InvalidInput(
            "channel name must be between 1 and 100 characters",
        ))
    }
}

fn normalize_topic(topic: Option<String>) -> Result<Option<String>, ChannelError> {
    let Some(topic) = topic else {
        return Ok(None);
    };
    let topic = topic.split_whitespace().collect::<Vec<_>>().join(" ");

    if topic.len() > 1024 {
        Err(ChannelError::InvalidInput(
            "channel topic must be 1024 characters or fewer",
        ))
    } else if topic.is_empty() {
        Ok(None)
    } else {
        Ok(Some(topic))
    }
}

fn slugify(name: &str) -> Result<String, ChannelError> {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        Err(ChannelError::InvalidInput(
            "channel name must include letters or numbers",
        ))
    } else {
        Ok(slug)
    }
}
