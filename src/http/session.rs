use axum::http::{HeaderMap, header};

use crate::domain::auth::AuthError;

pub fn bearer_token(headers: &HeaderMap) -> Result<&str, AuthError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(AuthError::Unauthorized);
    };

    let value = value.to_str().map_err(|_| AuthError::Unauthorized)?;
    value
        .strip_prefix("Bearer ")
        .filter(|token| !token.trim().is_empty())
        .map(str::trim)
        .ok_or(AuthError::Unauthorized)
}
