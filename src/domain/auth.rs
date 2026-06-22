use argon2::Argon2;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct StoredUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
}

#[derive(Clone, Debug)]
pub struct StoredSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
}

#[derive(Debug)]
pub struct AuthResult {
    pub user: AuthUser,
    pub session_token: String,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    EmailAlreadyRegistered,
    InvalidCredentials,
    Unauthorized,
    StoreUnavailable,
}

impl AuthError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::EmailAlreadyRegistered => StatusCode::CONFLICT,
            Self::InvalidCredentials | Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::EmailAlreadyRegistered => "email_already_registered",
            Self::InvalidCredentials => "invalid_credentials",
            Self::Unauthorized => "unauthorized",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::EmailAlreadyRegistered => "email is already registered",
            Self::InvalidCredentials => "email or password is incorrect",
            Self::Unauthorized => "valid bearer session is required",
            Self::StoreUnavailable => "auth store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait AuthStore: Send + Sync {
    async fn create_user(&self, user: StoredUser) -> Result<(), AuthError>;
    async fn find_user_by_email(&self, email: &str) -> Result<Option<StoredUser>, AuthError>;
    async fn create_session(&self, session: StoredSession) -> Result<(), AuthError>;
    async fn find_user_by_session_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<StoredUser>, AuthError>;
    async fn revoke_session(&self, token_hash: &str) -> Result<(), AuthError>;
}

#[derive(Clone)]
pub struct AuthService {
    store: std::sync::Arc<dyn AuthStore>,
}

impl AuthService {
    pub fn new(store: std::sync::Arc<dyn AuthStore>) -> Self {
        Self { store }
    }

    pub async fn register(
        &self,
        email: String,
        display_name: String,
        password: String,
    ) -> Result<AuthResult, AuthError> {
        let email = normalize_email(email)?;
        let display_name = normalize_display_name(display_name)?;
        validate_password(&password)?;

        if self.store.find_user_by_email(&email).await?.is_some() {
            return Err(AuthError::EmailAlreadyRegistered);
        }

        let user = StoredUser {
            id: ids::new_uuid_v7(),
            email,
            display_name,
            password_hash: hash_password_async(password).await?,
        };

        self.store.create_user(user.clone()).await?;
        self.create_session_for_user(user).await
    }

    pub async fn login(&self, email: String, password: String) -> Result<AuthResult, AuthError> {
        let email = normalize_email(email)?;
        let Some(user) = self.store.find_user_by_email(&email).await? else {
            return Err(AuthError::InvalidCredentials);
        };

        if !verify_password_async(password, user.password_hash.clone()).await? {
            return Err(AuthError::InvalidCredentials);
        }

        self.create_session_for_user(user).await
    }

    pub async fn logout(&self, token: &str) -> Result<(), AuthError> {
        self.store.revoke_session(&hash_session_token(token)).await
    }

    pub async fn user_for_token(&self, token: &str) -> Result<AuthUser, AuthError> {
        self.store
            .find_user_by_session_token_hash(&hash_session_token(token))
            .await?
            .map(AuthUser::from)
            .ok_or(AuthError::Unauthorized)
    }

    async fn create_session_for_user(&self, user: StoredUser) -> Result<AuthResult, AuthError> {
        let token = generate_session_token();
        self.store
            .create_session(StoredSession {
                id: ids::new_uuid_v7(),
                user_id: user.id,
                token_hash: hash_session_token(&token),
            })
            .await?;

        Ok(AuthResult {
            user: AuthUser::from(user),
            session_token: token,
        })
    }
}

impl From<StoredUser> for AuthUser {
    fn from(user: StoredUser) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
        }
    }
}

fn normalize_email(email: String) -> Result<String, AuthError> {
    let email = email.trim().to_ascii_lowercase();
    let valid_shape = email.len() <= 320
        && email.contains('@')
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.contains(' ');

    if valid_shape {
        Ok(email)
    } else {
        Err(AuthError::InvalidInput("valid email is required"))
    }
}

fn normalize_display_name(display_name: String) -> Result<String, AuthError> {
    let display_name = display_name.trim().to_owned();
    if (1..=80).contains(&display_name.len()) {
        Ok(display_name)
    } else {
        Err(AuthError::InvalidInput(
            "display name must be between 1 and 80 characters",
        ))
    }
}

fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.len() >= 12 {
        Ok(())
    } else {
        Err(AuthError::InvalidInput(
            "password must be at least 12 characters",
        ))
    }
}

async fn hash_password_async(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|_| AuthError::StoreUnavailable)?
}

async fn verify_password_async(password: String, password_hash: String) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || verify_password(&password, &password_hash))
        .await
        .map_err(|_| AuthError::StoreUnavailable)
}

fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::StoreUnavailable)
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

fn generate_session_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_session_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
