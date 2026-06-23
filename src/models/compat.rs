use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateCompatMessageRequest {
    pub content: String,
    pub tts: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PatchCompatMessageRequest {
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompatUserResponse {
    pub id: String,
    pub username: String,
    pub bot: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompatGuildResponse {
    pub id: String,
    pub name: String,
    pub unavailable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompatChannelResponse {
    pub id: String,
    pub guild_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: i32,
    pub position: i32,
    pub topic: Option<String>,
    pub nsfw: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompatMessageResponse {
    pub id: String,
    pub channel_id: String,
    pub author: CompatUserResponse,
    pub content: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<CompatUserResponse>,
    pub mention_roles: Vec<String>,
    pub attachments: Vec<serde_json::Value>,
    pub embeds: Vec<serde_json::Value>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub kind: i32,
}

#[derive(Debug, Serialize)]
pub struct CompatErrorResponse {
    pub message: &'static str,
    pub code: i32,
}
