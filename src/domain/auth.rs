use argon2::Argon2;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::http::StatusCode;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::ids;

type HmacSha256 = Hmac<Sha256>;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOidcProvider {
    pub organization_id: Uuid,
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub client_id: String,
    pub client_secret: String,
    pub allowed_domains: Vec<String>,
    pub require_sso: bool,
    pub auto_join_role: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OidcProvider {
    pub organization_id: Uuid,
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub client_id: String,
    pub allowed_domains: Vec<String>,
    pub require_sso: bool,
    pub auto_join_role: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOidcIdentity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub email: String,
}

#[derive(Debug)]
pub struct ConfigureOidcProviderInput {
    pub organization_id: Uuid,
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub client_id: String,
    pub client_secret: String,
    pub allowed_domains: Vec<String>,
    pub require_sso: bool,
    pub auto_join_role: String,
}

#[derive(Debug)]
pub struct OidcProviderAssertion {
    pub issuer: String,
    pub subject: String,
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
    pub signature: String,
}

#[derive(Debug)]
pub struct AuthResult {
    pub user: AuthUser,
    pub session_token: String,
}

#[derive(Debug)]
pub struct OidcLoginResult {
    pub auth: AuthResult,
    pub organization_id: Uuid,
    pub auto_join_role: String,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidInput(&'static str),
    EmailAlreadyRegistered,
    InvalidCredentials,
    Unauthorized,
    SsoRequired,
    StoreUnavailable,
}

impl AuthError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::EmailAlreadyRegistered => StatusCode::CONFLICT,
            Self::InvalidCredentials | Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::SsoRequired => StatusCode::FORBIDDEN,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::EmailAlreadyRegistered => "email_already_registered",
            Self::InvalidCredentials => "invalid_credentials",
            Self::Unauthorized => "unauthorized",
            Self::SsoRequired => "sso_required",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::EmailAlreadyRegistered => "email is already registered",
            Self::InvalidCredentials => "email or password is incorrect",
            Self::Unauthorized => "valid bearer session is required",
            Self::SsoRequired => "single sign-on is required for this email domain",
            Self::StoreUnavailable => "auth store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait AuthStore: Send + Sync {
    async fn create_user(&self, user: StoredUser) -> Result<(), AuthError>;
    async fn find_user_by_id(&self, user_id: Uuid) -> Result<Option<StoredUser>, AuthError>;
    async fn find_user_by_email(&self, email: &str) -> Result<Option<StoredUser>, AuthError>;
    async fn create_session(&self, session: StoredSession) -> Result<(), AuthError>;
    async fn find_user_by_session_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<StoredUser>, AuthError>;
    async fn revoke_session(&self, token_hash: &str) -> Result<(), AuthError>;
    async fn upsert_oidc_provider(&self, provider: StoredOidcProvider) -> Result<(), AuthError>;
    async fn oidc_provider_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<StoredOidcProvider>, AuthError>;
    async fn oidc_providers_for_email_domain(
        &self,
        domain: &str,
    ) -> Result<Vec<StoredOidcProvider>, AuthError>;
    async fn oidc_provider_for_issuer_and_domain(
        &self,
        issuer: &str,
        domain: &str,
    ) -> Result<Option<StoredOidcProvider>, AuthError>;
    async fn find_oidc_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<StoredOidcIdentity>, AuthError>;
    async fn create_oidc_identity(&self, identity: StoredOidcIdentity) -> Result<(), AuthError>;
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

        if self.sso_required_for_email(&email).await? {
            return Err(AuthError::SsoRequired);
        }

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
        if self.sso_required_for_email(&email).await? {
            return Err(AuthError::SsoRequired);
        }

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

    pub async fn user_by_id(&self, user_id: Uuid) -> Result<Option<AuthUser>, AuthError> {
        Ok(self
            .store
            .find_user_by_id(user_id)
            .await?
            .map(AuthUser::from))
    }

    pub async fn configure_oidc_provider(
        &self,
        input: ConfigureOidcProviderInput,
    ) -> Result<OidcProvider, AuthError> {
        let provider = StoredOidcProvider {
            organization_id: input.organization_id,
            issuer: normalize_url(input.issuer, "OIDC issuer is required")?,
            authorization_endpoint: normalize_url(
                input.authorization_endpoint,
                "OIDC authorization endpoint is required",
            )?,
            token_endpoint: normalize_url(input.token_endpoint, "OIDC token endpoint is required")?,
            jwks_uri: normalize_url(input.jwks_uri, "OIDC JWKS URI is required")?,
            client_id: normalize_non_empty(input.client_id, "OIDC client_id is required")?,
            client_secret: normalize_client_secret(input.client_secret)?,
            allowed_domains: normalize_allowed_domains(input.allowed_domains)?,
            require_sso: input.require_sso,
            auto_join_role: normalize_auto_join_role(input.auto_join_role)?,
        };

        self.store.upsert_oidc_provider(provider.clone()).await?;
        Ok(OidcProvider::from(provider))
    }

    pub async fn oidc_provider_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<OidcProvider>, AuthError> {
        Ok(self
            .store
            .oidc_provider_for_organization(organization_id)
            .await?
            .map(OidcProvider::from))
    }

    pub async fn oidc_providers_for_email(
        &self,
        email: String,
    ) -> Result<Vec<OidcProvider>, AuthError> {
        let email = normalize_email(email)?;
        let domain = email_domain(&email)?;
        Ok(self
            .store
            .oidc_providers_for_email_domain(domain)
            .await?
            .into_iter()
            .map(OidcProvider::from)
            .collect())
    }

    pub async fn oidc_login(
        &self,
        assertion: OidcProviderAssertion,
    ) -> Result<OidcLoginResult, AuthError> {
        if !assertion.email_verified {
            return Err(AuthError::InvalidCredentials);
        }

        let issuer = normalize_url(assertion.issuer, "OIDC issuer is required")?;
        let subject = normalize_non_empty(assertion.subject, "OIDC subject is required")?;
        let email = normalize_email(assertion.email)?;
        let display_name = normalize_display_name(assertion.display_name)?;
        let signature = assertion.signature.trim();
        if signature.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }

        let domain = email_domain(&email)?;
        let Some(provider) = self
            .store
            .oidc_provider_for_issuer_and_domain(&issuer, domain)
            .await?
        else {
            return Err(AuthError::InvalidCredentials);
        };

        let expected_signature =
            oidc_assertion_signature(&provider.client_secret, &issuer, &subject, &email, true)?;
        if !constant_time_eq(signature.as_bytes(), expected_signature.as_bytes()) {
            return Err(AuthError::InvalidCredentials);
        }

        let user = if let Some(identity) = self.store.find_oidc_identity(&issuer, &subject).await? {
            self.store
                .find_user_by_id(identity.user_id)
                .await?
                .ok_or(AuthError::InvalidCredentials)?
        } else if let Some(user) = self.store.find_user_by_email(&email).await? {
            self.store
                .create_oidc_identity(StoredOidcIdentity {
                    id: ids::new_uuid_v7(),
                    user_id: user.id,
                    organization_id: provider.organization_id,
                    issuer: issuer.clone(),
                    subject: subject.clone(),
                    email: email.clone(),
                })
                .await?;
            user
        } else {
            let user = StoredUser {
                id: ids::new_uuid_v7(),
                email: email.clone(),
                display_name,
                password_hash: oidc_password_marker(&issuer, &subject),
            };
            self.store.create_user(user.clone()).await?;
            self.store
                .create_oidc_identity(StoredOidcIdentity {
                    id: ids::new_uuid_v7(),
                    user_id: user.id,
                    organization_id: provider.organization_id,
                    issuer: issuer.clone(),
                    subject: subject.clone(),
                    email,
                })
                .await?;
            user
        };

        Ok(OidcLoginResult {
            auth: self.create_session_for_user(user).await?,
            organization_id: provider.organization_id,
            auto_join_role: provider.auto_join_role,
        })
    }

    async fn sso_required_for_email(&self, email: &str) -> Result<bool, AuthError> {
        let domain = email_domain(email)?;
        Ok(self
            .store
            .oidc_providers_for_email_domain(domain)
            .await?
            .into_iter()
            .any(|provider| provider.require_sso))
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

impl From<StoredOidcProvider> for OidcProvider {
    fn from(provider: StoredOidcProvider) -> Self {
        Self {
            organization_id: provider.organization_id,
            issuer: provider.issuer,
            authorization_endpoint: provider.authorization_endpoint,
            token_endpoint: provider.token_endpoint,
            jwks_uri: provider.jwks_uri,
            client_id: provider.client_id,
            allowed_domains: provider.allowed_domains,
            require_sso: provider.require_sso,
            auto_join_role: provider.auto_join_role,
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

fn email_domain(email: &str) -> Result<&str, AuthError> {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain)
        .filter(|domain| !domain.is_empty())
        .ok_or(AuthError::InvalidInput("valid email is required"))
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

fn normalize_url(url: String, message: &'static str) -> Result<String, AuthError> {
    let url = url.trim().trim_end_matches('/').to_owned();
    let valid = url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1");

    if valid {
        Ok(url)
    } else {
        Err(AuthError::InvalidInput(message))
    }
}

fn normalize_non_empty(value: String, message: &'static str) -> Result<String, AuthError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(AuthError::InvalidInput(message))
    } else {
        Ok(value)
    }
}

fn normalize_client_secret(secret: String) -> Result<String, AuthError> {
    let secret = normalize_non_empty(secret, "OIDC client_secret is required")?;
    if secret.len() < 8 {
        Err(AuthError::InvalidInput(
            "OIDC client_secret must be at least 8 characters",
        ))
    } else {
        Ok(secret)
    }
}

fn normalize_allowed_domains(domains: Vec<String>) -> Result<Vec<String>, AuthError> {
    let mut domains = domains
        .into_iter()
        .map(|domain| domain.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect::<Vec<_>>();
    domains.sort();
    domains.dedup();

    let valid = !domains.is_empty()
        && domains.iter().all(|domain| {
            domain.len() <= 253
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && domain
                    .split('.')
                    .all(|label| !label.is_empty() && label.len() <= 63)
        });

    if valid {
        Ok(domains)
    } else {
        Err(AuthError::InvalidInput(
            "OIDC allowed_domains must contain valid domains",
        ))
    }
}

fn normalize_auto_join_role(role: String) -> Result<String, AuthError> {
    let role = role.trim().to_ascii_lowercase();
    match role.as_str() {
        "member" | "admin" => Ok(role),
        _ => Err(AuthError::InvalidInput(
            "OIDC auto_join_role must be member or admin",
        )),
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

fn oidc_password_marker(issuer: &str, subject: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(issuer.as_bytes());
    hasher.update(b"\n");
    hasher.update(subject.as_bytes());
    format!("!oidc:{}", hex::encode(hasher.finalize()))
}

pub fn oidc_assertion_signature(
    secret: &str,
    issuer: &str,
    subject: &str,
    email: &str,
    email_verified: bool,
) -> Result<String, AuthError> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthError::StoreUnavailable)?;
    mac.update(format!("{issuer}\n{subject}\n{email}\n{email_verified}").as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right.iter())
        .fold(0_u8, |accumulator, (left, right)| {
            accumulator | (left ^ right)
        })
        == 0
}

pub fn hash_session_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
