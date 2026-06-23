use serde::Serialize;

use crate::domain::data_export::DataExport;
use crate::domain::message::Message;
use crate::models::attachment::AttachmentResponse;

#[derive(Debug, Serialize)]
pub struct DataExportEnvelope {
    pub export: DataExportResponse,
}

#[derive(Debug, Serialize)]
pub struct DataExportResponse {
    pub organization_id: String,
    pub format: String,
    pub from: String,
    pub to: String,
    pub messages: Vec<DataExportMessageResponse>,
    pub files: Vec<AttachmentResponse>,
}

#[derive(Debug, Serialize)]
pub struct DataExportMessageResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: Option<String>,
    pub channel_id: String,
    pub author_user_id: String,
    pub content: String,
    pub content_format: String,
    pub embeds: Vec<serde_json::Value>,
    pub components: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_avatar_url: Option<String>,
    pub edited_at: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
}

impl DataExportResponse {
    pub fn from_export(export: DataExport, files: Vec<AttachmentResponse>) -> Self {
        Self {
            organization_id: export.organization_id.to_string(),
            format: export.format,
            from: export.from,
            to: export.to,
            messages: export
                .messages
                .into_iter()
                .map(DataExportMessageResponse::from)
                .collect(),
            files,
        }
    }
}

impl From<Message> for DataExportMessageResponse {
    fn from(message: Message) -> Self {
        Self {
            id: message.id.to_string(),
            organization_id: message.organization_id.to_string(),
            space_id: message.space_id.map(|id| id.to_string()),
            channel_id: message.channel_id.to_string(),
            author_user_id: message.author_user_id.to_string(),
            content: message.content,
            content_format: message.content_format,
            embeds: message.embeds,
            components: message.components,
            webhook_username: message.webhook_username,
            webhook_avatar_url: message.webhook_avatar_url,
            edited_at: message.edited_at,
            deleted_at: message.deleted_at,
            created_at: message.created_at,
        }
    }
}
