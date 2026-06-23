use serde::{Deserialize, Serialize};

use crate::domain::push::PushToken;

#[derive(Debug, Deserialize)]
pub struct RegisterPushTokenRequest {
    pub platform: String,
    pub token: String,
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PushTokenResponse {
    pub id: String,
    pub user_id: String,
    pub platform: String,
    pub token_last_four: String,
    pub device_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PushTokenResourceResponse {
    pub push_token: PushTokenResponse,
}

#[derive(Debug, Serialize)]
pub struct PushTokenListResponse {
    pub push_tokens: Vec<PushTokenResponse>,
}

impl From<PushToken> for PushTokenResponse {
    fn from(token: PushToken) -> Self {
        Self {
            id: token.id.to_string(),
            user_id: token.user_id.to_string(),
            platform: token.platform.as_str().to_owned(),
            token_last_four: token.token_last_four,
            device_name: token.device_name,
            created_at: token.created_at,
            updated_at: token.updated_at,
        }
    }
}
