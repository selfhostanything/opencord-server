use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::ids;

pub const MAX_ATTACHMENT_SIZE_BYTES: i64 = 10 * 1024 * 1024;
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attachment {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub message_id: Option<Uuid>,
    pub uploader_user_id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: AttachmentStatus,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAttachment {
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub uploader_user_id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttachmentStatus {
    Pending,
    Uploaded,
    Linked,
}

impl AttachmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Uploaded => "uploaded",
            Self::Linked => "linked",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AttachmentError> {
        match value {
            "pending" => Ok(Self::Pending),
            "uploaded" => Ok(Self::Uploaded),
            "linked" => Ok(Self::Linked),
            _ => Err(AttachmentError::StoreUnavailable),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentContent {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum AttachmentError {
    InvalidInput(&'static str),
    NotFound,
    StoreUnavailable,
}

impl AttachmentError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::NotFound => "attachment_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::NotFound => "attachment was not found",
            Self::StoreUnavailable => "attachment store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait AttachmentStore: Send + Sync {
    async fn create_attachment(&self, attachment: Attachment) -> Result<(), AttachmentError>;
    async fn get_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<Attachment>, AttachmentError>;
    async fn upload_content(
        &self,
        attachment: Attachment,
        content: AttachmentContent,
    ) -> Result<Attachment, AttachmentError>;
    async fn content_for_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentContent>, AttachmentError>;
    async fn link_attachments_to_message(
        &self,
        message_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError>;
    async fn list_for_message_ids(
        &self,
        message_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError>;
    async fn stored_bytes_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<i64, AttachmentError>;
    async fn purge_for_retention(
        &self,
        organization_id: Uuid,
        created_before: Option<String>,
        dry_run: bool,
    ) -> Result<usize, AttachmentError>;
}

#[derive(Clone)]
pub struct AttachmentService {
    store: std::sync::Arc<dyn AttachmentStore>,
}

impl AttachmentService {
    pub fn new(store: std::sync::Arc<dyn AttachmentStore>) -> Self {
        Self { store }
    }

    pub async fn create_pending(
        &self,
        input: NewAttachment,
    ) -> Result<Attachment, AttachmentError> {
        let attachment = Attachment {
            id: ids::new_uuid_v7(),
            organization_id: input.organization_id,
            space_id: input.space_id,
            channel_id: input.channel_id,
            message_id: None,
            uploader_user_id: input.uploader_user_id,
            file_name: normalize_file_name(input.file_name)?,
            content_type: normalize_content_type(input.content_type)?,
            size_bytes: normalize_size(input.size_bytes)?,
            status: AttachmentStatus::Pending,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };

        self.store.create_attachment(attachment.clone()).await?;
        Ok(attachment)
    }

    pub async fn get(&self, attachment_id: Uuid) -> Result<Attachment, AttachmentError> {
        self.store
            .get_attachment(attachment_id)
            .await?
            .ok_or(AttachmentError::NotFound)
    }

    pub async fn upload(
        &self,
        attachment: Attachment,
        uploader_user_id: Uuid,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<Attachment, AttachmentError> {
        if attachment.uploader_user_id != uploader_user_id {
            return Err(AttachmentError::NotFound);
        }

        if attachment.status == AttachmentStatus::Linked {
            return Err(AttachmentError::InvalidInput(
                "attachment is already linked to a message",
            ));
        }

        if bytes.len() as i64 != attachment.size_bytes {
            return Err(AttachmentError::InvalidInput(
                "attachment upload size must match presign size",
            ));
        }

        let content_type = normalize_content_type(content_type)?;
        if content_type != attachment.content_type {
            return Err(AttachmentError::InvalidInput(
                "attachment content type must match presign content type",
            ));
        }

        self.store
            .upload_content(
                attachment,
                AttachmentContent {
                    content_type,
                    bytes,
                },
            )
            .await
    }

    pub async fn content(&self, attachment_id: Uuid) -> Result<AttachmentContent, AttachmentError> {
        self.store
            .content_for_attachment(attachment_id)
            .await?
            .ok_or(AttachmentError::NotFound)
    }

    pub async fn validate_for_message(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        channel_id: Uuid,
        uploader_user_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<(), AttachmentError> {
        if attachment_ids.len() > MAX_ATTACHMENTS_PER_MESSAGE {
            return Err(AttachmentError::InvalidInput(
                "message cannot include more than 10 attachments",
            ));
        }

        let mut seen = std::collections::HashSet::new();
        for attachment_id in attachment_ids {
            if !seen.insert(*attachment_id) {
                return Err(AttachmentError::InvalidInput(
                    "message attachment ids must be unique",
                ));
            }

            let attachment = self.get(*attachment_id).await?;
            if attachment.organization_id != organization_id
                || attachment.space_id != space_id
                || attachment.channel_id != channel_id
                || attachment.uploader_user_id != uploader_user_id
                || attachment.message_id.is_some()
                || attachment.status != AttachmentStatus::Uploaded
            {
                return Err(AttachmentError::InvalidInput(
                    "attachments must be uploaded files from the same channel",
                ));
            }
        }

        Ok(())
    }

    pub async fn link_to_message(
        &self,
        message_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.store
            .link_attachments_to_message(message_id, attachment_ids)
            .await
    }

    pub async fn list_for_message_ids(
        &self,
        message_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        self.store.list_for_message_ids(message_ids).await
    }
}

fn normalize_file_name(file_name: String) -> Result<String, AttachmentError> {
    let file_name = file_name.trim().to_owned();
    if file_name.is_empty() || file_name.len() > 255 || file_name.contains('/') {
        Err(AttachmentError::InvalidInput(
            "attachment file name must be 1 to 255 characters without path separators",
        ))
    } else {
        Ok(file_name)
    }
}

fn normalize_content_type(content_type: String) -> Result<String, AttachmentError> {
    let content_type = content_type.trim().to_ascii_lowercase();
    if content_type.is_empty() || content_type.len() > 120 || !content_type.contains('/') {
        Err(AttachmentError::InvalidInput(
            "attachment content type must be a valid media type",
        ))
    } else {
        Ok(content_type)
    }
}

fn normalize_size(size_bytes: i64) -> Result<i64, AttachmentError> {
    if (1..=MAX_ATTACHMENT_SIZE_BYTES).contains(&size_bytes) {
        Ok(size_bytes)
    } else {
        Err(AttachmentError::InvalidInput(
            "attachment size must be between 1 byte and 10 MiB",
        ))
    }
}
