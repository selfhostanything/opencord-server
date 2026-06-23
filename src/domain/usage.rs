use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::attachment::AttachmentStore;
use crate::domain::calendar_sync::CalendarStore;
use crate::domain::organization::OrganizationStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageSummary {
    pub organization_id: Uuid,
    pub active_users: i64,
    pub stored_file_bytes: i64,
    pub calendar_connected_accounts: i64,
}

#[derive(Debug)]
pub enum UsageError {
    StoreUnavailable,
}

impl UsageError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::StoreUnavailable => "usage_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::StoreUnavailable => "usage store is unavailable",
        }
    }
}

#[derive(Clone)]
pub struct UsageService {
    organizations: std::sync::Arc<dyn OrganizationStore>,
    attachments: std::sync::Arc<dyn AttachmentStore>,
    calendars: std::sync::Arc<dyn CalendarStore>,
}

impl UsageService {
    pub fn new(
        organizations: std::sync::Arc<dyn OrganizationStore>,
        attachments: std::sync::Arc<dyn AttachmentStore>,
        calendars: std::sync::Arc<dyn CalendarStore>,
    ) -> Self {
        Self {
            organizations,
            attachments,
            calendars,
        }
    }

    pub async fn summary(&self, organization_id: Uuid) -> Result<UsageSummary, UsageError> {
        let active_member_user_ids = self
            .organizations
            .active_member_user_ids(organization_id)
            .await
            .map_err(|_| UsageError::StoreUnavailable)?;
        let stored_file_bytes = self
            .attachments
            .stored_bytes_for_organization(organization_id)
            .await
            .map_err(|_| UsageError::StoreUnavailable)?;
        let calendar_connected_accounts = self
            .calendars
            .count_accounts_for_user_ids(&active_member_user_ids)
            .await
            .map_err(|_| UsageError::StoreUnavailable)?;

        Ok(UsageSummary {
            organization_id,
            active_users: active_member_user_ids.len() as i64,
            stored_file_bytes,
            calendar_connected_accounts,
        })
    }
}
