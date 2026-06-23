use serde::Serialize;

use crate::domain::usage::UsageSummary;

#[derive(Clone, Debug, Serialize)]
pub struct UsageSummaryResponse {
    pub organization_id: String,
    pub active_users: i64,
    pub stored_file_bytes: i64,
    pub calendar_connected_accounts: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageResourceResponse {
    pub usage: UsageSummaryResponse,
}

impl From<UsageSummary> for UsageSummaryResponse {
    fn from(summary: UsageSummary) -> Self {
        Self {
            organization_id: summary.organization_id.to_string(),
            active_users: summary.active_users,
            stored_file_bytes: summary.stored_file_bytes,
            calendar_connected_accounts: summary.calendar_connected_accounts,
        }
    }
}
