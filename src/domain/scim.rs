use std::sync::Arc;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthStore, StoredUser};
use crate::domain::ids;
use crate::domain::organization::{OrganizationError, OrganizationStore, StoredOrganizationMember};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredScimToken {
    pub organization_id: Uuid,
    pub token_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredScimUser {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub external_id: String,
    pub user_name: String,
    pub display_name: String,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScimToken {
    pub organization_id: Uuid,
    pub token: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScimUser {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub external_id: String,
    pub user_name: String,
    pub display_name: String,
    pub active: bool,
}

#[derive(Debug)]
pub struct ProvisionScimUserInput {
    pub external_id: String,
    pub user_name: String,
    pub display_name: Option<String>,
    pub active: bool,
}

#[derive(Debug)]
pub enum ScimError {
    InvalidInput(&'static str),
    Unauthorized,
    NotFound,
    StoreUnavailable,
}

impl ScimError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "scim_user_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::Unauthorized => "valid SCIM bearer token is required",
            Self::NotFound => "SCIM user was not found",
            Self::StoreUnavailable => "SCIM store is unavailable",
        }
    }
}

impl From<AuthError> for ScimError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidInput(message) => Self::InvalidInput(message),
            AuthError::Unauthorized | AuthError::InvalidCredentials => Self::Unauthorized,
            _ => Self::StoreUnavailable,
        }
    }
}

impl From<OrganizationError> for ScimError {
    fn from(error: OrganizationError) -> Self {
        match error {
            OrganizationError::InvalidInput(message) => Self::InvalidInput(message),
            OrganizationError::NotFound => Self::NotFound,
            _ => Self::StoreUnavailable,
        }
    }
}

#[async_trait::async_trait]
pub trait ScimStore: Send + Sync {
    async fn rotate_token(&self, token: StoredScimToken) -> Result<(), ScimError>;
    async fn token_by_hash(&self, token_hash: &str) -> Result<Option<StoredScimToken>, ScimError>;
    async fn upsert_user(&self, user: StoredScimUser) -> Result<StoredScimUser, ScimError>;
    async fn user_by_external_id(
        &self,
        organization_id: Uuid,
        external_id: &str,
    ) -> Result<Option<StoredScimUser>, ScimError>;
    async fn set_user_active(
        &self,
        organization_id: Uuid,
        external_id: &str,
        active: bool,
    ) -> Result<StoredScimUser, ScimError>;
}

#[derive(Clone)]
pub struct ScimService {
    store: Arc<dyn ScimStore>,
    auth: Arc<dyn AuthStore>,
    organizations: Arc<dyn OrganizationStore>,
}

impl ScimService {
    pub fn new(
        store: Arc<dyn ScimStore>,
        auth: Arc<dyn AuthStore>,
        organizations: Arc<dyn OrganizationStore>,
    ) -> Self {
        Self {
            store,
            auth,
            organizations,
        }
    }

    pub async fn rotate_token(&self, organization_id: Uuid) -> Result<ScimToken, ScimError> {
        let token = generate_scim_token();
        self.store
            .rotate_token(StoredScimToken {
                organization_id,
                token_hash: hash_scim_token(&token),
            })
            .await?;

        Ok(ScimToken {
            organization_id,
            token,
        })
    }

    pub async fn provision_user(
        &self,
        token: &str,
        input: ProvisionScimUserInput,
    ) -> Result<ScimUser, ScimError> {
        let config = self.config_for_token(token).await?;
        let external_id = normalize_external_id(input.external_id)?;
        let user_name = normalize_email(input.user_name)?;
        let display_name = normalize_display_name(input.display_name, &user_name)?;
        let user = if let Some(existing) = self.auth.find_user_by_email(&user_name).await? {
            existing
        } else {
            let user = StoredUser {
                id: ids::new_uuid_v7(),
                email: user_name.clone(),
                display_name: display_name.clone(),
                password_hash: scim_password_marker(&config.organization_id, &external_id),
            };
            self.auth.create_user(user.clone()).await?;
            user
        };

        self.organizations
            .add_member_if_missing(StoredOrganizationMember {
                organization_id: config.organization_id,
                user_id: user.id,
                role: "member".to_owned(),
                status: "active".to_owned(),
            })
            .await?;
        if !input.active {
            self.organizations
                .set_member_status(config.organization_id, user.id, "inactive".to_owned())
                .await?;
        }

        let scim_user = self
            .store
            .upsert_user(StoredScimUser {
                id: ids::new_uuid_v7(),
                organization_id: config.organization_id,
                user_id: user.id,
                external_id,
                user_name,
                display_name,
                active: input.active,
            })
            .await?;

        Ok(ScimUser::from(scim_user))
    }

    pub async fn get_user(&self, token: &str, external_id: String) -> Result<ScimUser, ScimError> {
        let config = self.config_for_token(token).await?;
        let external_id = normalize_external_id(external_id)?;
        self.store
            .user_by_external_id(config.organization_id, &external_id)
            .await?
            .map(ScimUser::from)
            .ok_or(ScimError::NotFound)
    }

    pub async fn set_user_active(
        &self,
        token: &str,
        external_id: String,
        active: bool,
    ) -> Result<ScimUser, ScimError> {
        let config = self.config_for_token(token).await?;
        let external_id = normalize_external_id(external_id)?;
        let user = self
            .store
            .set_user_active(config.organization_id, &external_id, active)
            .await?;
        self.organizations
            .set_member_status(
                config.organization_id,
                user.user_id,
                if active { "active" } else { "inactive" }.to_owned(),
            )
            .await?;

        Ok(ScimUser::from(user))
    }

    async fn config_for_token(&self, token: &str) -> Result<StoredScimToken, ScimError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(ScimError::Unauthorized);
        }

        self.store
            .token_by_hash(&hash_scim_token(token))
            .await?
            .ok_or(ScimError::Unauthorized)
    }
}

impl From<StoredScimUser> for ScimUser {
    fn from(user: StoredScimUser) -> Self {
        Self {
            id: user.id,
            organization_id: user.organization_id,
            user_id: user.user_id,
            external_id: user.external_id,
            user_name: user.user_name,
            display_name: user.display_name,
            active: user.active,
        }
    }
}

fn normalize_external_id(external_id: String) -> Result<String, ScimError> {
    let external_id = external_id.trim().to_owned();
    if (1..=200).contains(&external_id.len()) {
        Ok(external_id)
    } else {
        Err(ScimError::InvalidInput("SCIM externalId is required"))
    }
}

fn normalize_email(email: String) -> Result<String, ScimError> {
    let email = email.trim().to_ascii_lowercase();
    let valid_shape = email.len() <= 320
        && email.contains('@')
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.contains(' ');

    if valid_shape {
        Ok(email)
    } else {
        Err(ScimError::InvalidInput(
            "SCIM userName must be a valid email",
        ))
    }
}

fn normalize_display_name(
    display_name: Option<String>,
    fallback: &str,
) -> Result<String, ScimError> {
    let display_name = display_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned());
    if (1..=80).contains(&display_name.len()) {
        Ok(display_name)
    } else {
        Err(ScimError::InvalidInput(
            "SCIM display name must be between 1 and 80 characters",
        ))
    }
}

fn generate_scim_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("opc-scim-{}", hex::encode(bytes))
}

fn hash_scim_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn scim_password_marker(organization_id: &Uuid, external_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(organization_id.to_string().as_bytes());
    hasher.update(b"\n");
    hasher.update(external_id.as_bytes());
    format!("!scim:{}", hex::encode(hasher.finalize()))
}
