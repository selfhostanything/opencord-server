use serde::{Deserialize, Serialize};

use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct CreateIncomingWebhookRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteIncomingWebhookRequest {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub embeds: Vec<Value>,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IncomingWebhookResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub bot_user_id: String,
    pub created_by_user_id: String,
    pub name: String,
    pub status: String,
    pub token: String,
    pub token_last_four: String,
    pub execute_url: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct IncomingWebhookDetailResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub bot_user_id: String,
    pub created_by_user_id: String,
    pub name: String,
    pub status: String,
    pub token_last_four: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct IncomingWebhookResourceResponse {
    pub webhook: IncomingWebhookResponse,
}

#[derive(Debug, Serialize)]
pub struct IncomingWebhookListResponse {
    pub webhooks: Vec<IncomingWebhookDetailResponse>,
}
