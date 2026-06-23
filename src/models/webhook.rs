use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateIncomingWebhookRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteIncomingWebhookRequest {
    pub content: String,
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
pub struct IncomingWebhookResourceResponse {
    pub webhook: IncomingWebhookResponse,
}
