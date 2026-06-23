use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthStore, StoredUser};
use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotApplication {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub bot_user_id: Uuid,
    pub created_by_user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredBotToken {
    pub id: Uuid,
    pub application_id: Uuid,
    pub token_hash: String,
    pub token_last_four: String,
    pub created_by_user_id: Uuid,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotToken {
    pub id: Uuid,
    pub application_id: Uuid,
    pub token: String,
    pub token_last_four: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotApplicationCreated {
    pub application: BotApplication,
    pub token: BotToken,
}

#[derive(Debug)]
pub struct CreateBotApplicationInput {
    pub organization_id: Uuid,
    pub created_by_user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug)]
pub enum BotError {
    InvalidInput(&'static str),
    StoreUnavailable,
}

impl BotError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::StoreUnavailable => "bot_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::StoreUnavailable => "bot store is unavailable",
        }
    }
}

impl From<AuthError> for BotError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidInput(message) => Self::InvalidInput(message),
            _ => Self::StoreUnavailable,
        }
    }
}

#[async_trait::async_trait]
pub trait BotStore: Send + Sync {
    async fn create_application(
        &self,
        application: BotApplication,
        token: StoredBotToken,
    ) -> Result<(), BotError>;
}

#[derive(Clone)]
pub struct BotService {
    store: std::sync::Arc<dyn BotStore>,
    auth: std::sync::Arc<dyn AuthStore>,
}

impl BotService {
    pub fn new(store: std::sync::Arc<dyn BotStore>, auth: std::sync::Arc<dyn AuthStore>) -> Self {
        Self { store, auth }
    }

    pub async fn create_application(
        &self,
        input: CreateBotApplicationInput,
    ) -> Result<BotApplicationCreated, BotError> {
        let name = normalize_name(input.name)?;
        let description = normalize_description(input.description)?;
        let application_id = ids::new_uuid_v7();
        let bot_user_id = ids::new_uuid_v7();
        let bot_user = StoredUser {
            id: bot_user_id,
            email: bot_user_email(bot_user_id),
            display_name: name.clone(),
            password_hash: bot_user_password_marker(application_id),
        };
        self.auth.create_user(bot_user).await?;

        let application = BotApplication {
            id: application_id,
            organization_id: input.organization_id,
            bot_user_id,
            created_by_user_id: input.created_by_user_id,
            name,
            description,
            status: "active".to_owned(),
        };
        let token = generate_bot_token();
        let stored_token = StoredBotToken {
            id: ids::new_uuid_v7(),
            application_id,
            token_hash: hash_bot_token(&token),
            token_last_four: token_last_four(&token),
            created_by_user_id: input.created_by_user_id,
            active: true,
        };
        self.store
            .create_application(application.clone(), stored_token.clone())
            .await?;

        Ok(BotApplicationCreated {
            application,
            token: BotToken {
                id: stored_token.id,
                application_id: stored_token.application_id,
                token,
                token_last_four: stored_token.token_last_four,
            },
        })
    }
}

fn normalize_name(name: String) -> Result<String, BotError> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.len() > 80 {
        Err(BotError::InvalidInput(
            "bot application name must be 1 to 80 characters",
        ))
    } else {
        Ok(name)
    }
}

fn normalize_description(description: Option<String>) -> Result<Option<String>, BotError> {
    let Some(description) = description else {
        return Ok(None);
    };
    let description = description.trim().to_owned();
    if description.is_empty() {
        Ok(None)
    } else if description.len() > 500 {
        Err(BotError::InvalidInput(
            "bot application description must be at most 500 characters",
        ))
    } else {
        Ok(Some(description))
    }
}

fn generate_bot_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("ocb_{}", hex::encode(bytes))
}

fn hash_bot_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
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

fn bot_user_email(bot_user_id: Uuid) -> String {
    format!("bot-{}@bots.opencord.local", bot_user_id.simple())
}

fn bot_user_password_marker(application_id: Uuid) -> String {
    format!("bot-user:{application_id}")
}
