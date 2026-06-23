use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateCompatMessageRequest {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub embeds: Vec<serde_json::Value>,
    pub allowed_mentions: Option<serde_json::Value>,
    pub message_reference: Option<CompatMessageReferenceRequest>,
    pub tts: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CompatMessageReferenceRequest {
    pub message_id: Uuid,
    pub channel_id: Option<Uuid>,
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
pub struct CompatRoleResponse {
    pub id: String,
    pub name: String,
    pub color: i32,
    pub hoist: bool,
    pub position: i32,
    pub permissions: String,
    pub managed: bool,
    pub mentionable: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<CompatMessageReferenceResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_message: Option<Box<CompatMessageResponse>>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub kind: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompatMessageReferenceResponse {
    pub message_id: String,
    pub channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompatErrorResponse {
    pub message: &'static str,
    pub code: i32,
}
