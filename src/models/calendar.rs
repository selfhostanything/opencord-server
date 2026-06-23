use serde::{Deserialize, Serialize};

use crate::domain::calendar_sync::{
    CalendarEventSync, CalendarEventSyncResult, ConnectCalendarAccount, ConnectedCalendarAccount,
};

#[derive(Debug, Deserialize)]
pub struct ConnectGoogleCalendarRequest {
    pub external_account_id: String,
    pub calendar_id: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectMicrosoftCalendarRequest {
    pub external_account_id: String,
    pub calendar_id: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalendarAccountResponse {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub external_account_id: String,
    pub calendar_id: String,
    pub token_last_four: String,
    pub sync_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalendarEventResponse {
    pub id: String,
    pub meeting_id: String,
    pub account_id: String,
    pub provider: String,
    pub provider_event_id: String,
    pub provider_event_url: Option<String>,
    pub calendar_id: String,
    pub status: String,
    pub operation: String,
    pub last_synced_at: String,
    pub failure_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CalendarAccountResourceResponse {
    pub account: CalendarAccountResponse,
}

#[derive(Debug, Serialize)]
pub struct CalendarAccountListResponse {
    pub accounts: Vec<CalendarAccountResponse>,
}

#[derive(Debug, Serialize)]
pub struct CalendarEventResourceResponse {
    pub calendar_event: CalendarEventResponse,
}

impl From<ConnectGoogleCalendarRequest> for ConnectCalendarAccount {
    fn from(request: ConnectGoogleCalendarRequest) -> Self {
        Self {
            external_account_id: request.external_account_id,
            calendar_id: request.calendar_id.unwrap_or_else(|| "primary".to_owned()),
            access_token: request.access_token,
            refresh_token: request.refresh_token,
        }
    }
}

impl From<ConnectMicrosoftCalendarRequest> for ConnectCalendarAccount {
    fn from(request: ConnectMicrosoftCalendarRequest) -> Self {
        Self {
            external_account_id: request.external_account_id,
            calendar_id: request.calendar_id.unwrap_or_else(|| "primary".to_owned()),
            access_token: request.access_token,
            refresh_token: request.refresh_token,
        }
    }
}

impl From<ConnectedCalendarAccount> for CalendarAccountResponse {
    fn from(account: ConnectedCalendarAccount) -> Self {
        Self {
            id: account.id.to_string(),
            user_id: account.user_id.to_string(),
            provider: account.provider,
            external_account_id: account.external_account_id,
            calendar_id: account.calendar_id,
            token_last_four: account.token_last_four,
            sync_enabled: account.sync_enabled,
            created_at: account.created_at,
            updated_at: account.updated_at,
        }
    }
}

impl From<CalendarEventSyncResult> for CalendarEventResponse {
    fn from(result: CalendarEventSyncResult) -> Self {
        Self::from_event(result.event, result.operation)
    }
}

impl CalendarEventResponse {
    fn from_event(event: CalendarEventSync, operation: String) -> Self {
        Self {
            id: event.id.to_string(),
            meeting_id: event.meeting_id.to_string(),
            account_id: event.account_id.to_string(),
            provider: event.provider,
            provider_event_id: event.provider_event_id,
            provider_event_url: event.provider_event_url,
            calendar_id: event.calendar_id,
            status: event.status,
            operation,
            last_synced_at: event.last_synced_at,
            failure_reason: event.failure_reason,
            created_at: event.created_at,
            updated_at: event.updated_at,
        }
    }
}
