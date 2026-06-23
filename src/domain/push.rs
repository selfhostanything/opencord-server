use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: PushPlatform,
    pub token: String,
    pub token_hash: String,
    pub token_last_four: String,
    pub device_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PushPlatform {
    Ios,
    Android,
    Web,
    Desktop,
}

impl PushPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
            Self::Web => "web",
            Self::Desktop => "desktop",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PushError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ios" => Ok(Self::Ios),
            "android" => Ok(Self::Android),
            "web" => Ok(Self::Web),
            "desktop" => Ok(Self::Desktop),
            _ => Err(PushError::InvalidInput(
                "platform must be ios, android, web, or desktop",
            )),
        }
    }
}

#[derive(Debug)]
pub enum PushError {
    InvalidInput(&'static str),
    StoreUnavailable,
}

impl PushError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::StoreUnavailable => "push_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::StoreUnavailable => "push token store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait PushTokenStore: Send + Sync {
    async fn upsert_token(&self, token: PushToken) -> Result<PushToken, PushError>;
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<PushToken>, PushError>;
}

#[derive(Clone)]
pub struct PushService {
    store: std::sync::Arc<dyn PushTokenStore>,
}

impl PushService {
    pub fn new(store: std::sync::Arc<dyn PushTokenStore>) -> Self {
        Self { store }
    }

    pub async fn register(
        &self,
        user_id: Uuid,
        platform: String,
        token: String,
        device_name: Option<String>,
    ) -> Result<PushToken, PushError> {
        let platform = PushPlatform::parse(&platform)?;
        let token = normalize_token(token)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let push_token = PushToken {
            id: ids::new_uuid_v7(),
            user_id,
            platform,
            token_hash: hash_push_token(&token),
            token_last_four: token_last_four(&token),
            token,
            device_name: normalize_device_name(device_name)?,
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.upsert_token(push_token).await
    }

    pub async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<PushToken>, PushError> {
        self.store.list_for_user(user_id).await
    }
}

fn normalize_token(token: String) -> Result<String, PushError> {
    let token = token.trim().to_owned();
    if (8..=4096).contains(&token.len()) {
        Ok(token)
    } else {
        Err(PushError::InvalidInput(
            "device token must be between 8 and 4096 bytes",
        ))
    }
}

fn normalize_device_name(device_name: Option<String>) -> Result<Option<String>, PushError> {
    let Some(device_name) = device_name else {
        return Ok(None);
    };

    let device_name = device_name.trim().to_owned();
    if device_name.is_empty() {
        Ok(None)
    } else if device_name.len() <= 120 {
        Ok(Some(device_name))
    } else {
        Err(PushError::InvalidInput(
            "device name must be 120 characters or fewer",
        ))
    }
}

fn token_last_four(token: &str) -> String {
    token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn hash_push_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
