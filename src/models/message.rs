use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::attachment::AttachmentResponse;

#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub content: String,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct PatchMessageRequest {
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: Option<String>,
    pub channel_id: String,
    pub author_user_id: String,
    pub content: String,
    pub content_format: String,
    pub edited_at: Option<String>,
    pub deleted_at: Option<String>,
    pub attachments: Vec<AttachmentResponse>,
}

#[derive(Debug, Serialize)]
pub struct MessageResourceResponse {
    pub message: MessageResponse,
}

#[derive(Debug, Serialize)]
pub struct MessageListResponse {
    pub messages: Vec<MessageResponse>,
}
