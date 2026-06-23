use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct OidcProvidersQuery {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackRequest {
    pub issuer: String,
    pub subject: String,
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigureOidcProviderRequest {
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

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub session: SessionResponse,
}

#[derive(Debug, Serialize)]
pub struct OidcProviderResponse {
    pub organization_id: String,
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub client_id: String,
    pub allowed_domains: Vec<String>,
    pub require_sso: bool,
    pub auto_join_role: String,
}

#[derive(Debug, Serialize)]
pub struct OidcProvidersResponse {
    pub providers: Vec<OidcProviderResponse>,
}

#[derive(Debug, Serialize)]
pub struct OidcProviderEnvelope {
    pub provider: OidcProviderResponse,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: &'static str,
    pub message: &'static str,
}
