use std::sync::Arc;

use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::attachment::{Attachment, AttachmentError, AttachmentStore};
use crate::domain::message::{Message, MessageError, MessageStore};

#[derive(Clone, Debug, PartialEq)]
pub struct DataExport {
    pub organization_id: Uuid,
    pub format: String,
    pub from: String,
    pub to: String,
    pub messages: Vec<Message>,
    pub files: Vec<Attachment>,
}

#[derive(Debug)]
pub enum DataExportError {
    InvalidInput(&'static str),
    StoreUnavailable,
}

impl DataExportError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::StoreUnavailable => "data_export_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::StoreUnavailable => "data export store is unavailable",
        }
    }
}

impl From<MessageError> for DataExportError {
    fn from(error: MessageError) -> Self {
        match error {
            MessageError::InvalidInput(message) => Self::InvalidInput(message),
            MessageError::NotFound | MessageError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

impl From<AttachmentError> for DataExportError {
    fn from(error: AttachmentError) -> Self {
        match error {
            AttachmentError::InvalidInput(message) => Self::InvalidInput(message),
            AttachmentError::NotFound | AttachmentError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

#[derive(Clone)]
pub struct DataExportService {
    messages: Arc<dyn MessageStore>,
    attachments: Arc<dyn AttachmentStore>,
}

impl DataExportService {
    pub fn new(messages: Arc<dyn MessageStore>, attachments: Arc<dyn AttachmentStore>) -> Self {
        Self {
            messages,
            attachments,
        }
    }

    pub async fn export_for_organization(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<DataExport, DataExportError> {
        let from = normalize_rfc3339(from, "data export from must be RFC3339")?;
        let to = normalize_rfc3339(to, "data export to must be RFC3339")?;
        if from > to {
            return Err(DataExportError::InvalidInput(
                "data export from must be before to",
            ));
        }
        let from_string = from.to_rfc3339_opts(SecondsFormat::Millis, true);
        let to_string = to.to_rfc3339_opts(SecondsFormat::Millis, true);
        let messages = self
            .messages
            .list_for_organization_between(organization_id, from_string.clone(), to_string.clone())
            .await?;
        let message_ids = messages
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>();
        let files = self.attachments.list_for_message_ids(&message_ids).await?;

        Ok(DataExport {
            organization_id,
            format: "json".to_owned(),
            from: from_string,
            to: to_string,
            messages,
            files,
        })
    }
}

fn normalize_rfc3339(
    value: String,
    message: &'static str,
) -> Result<DateTime<Utc>, DataExportError> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| DataExportError::InvalidInput(message))
}
