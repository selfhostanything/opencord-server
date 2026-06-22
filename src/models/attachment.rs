use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PresignAttachmentRequest {
    pub channel_id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AttachmentResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub message_id: Option<String>,
    pub uploader_user_id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: String,
    pub download_url: String,
}

#[derive(Debug, Serialize)]
pub struct AttachmentUploadResponse {
    pub method: &'static str,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct AttachmentPresignResponse {
    pub attachment: AttachmentResponse,
    pub upload: AttachmentUploadResponse,
}

#[derive(Debug, Serialize)]
pub struct AttachmentResourceResponse {
    pub attachment: AttachmentResponse,
}
