use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::ids;
use crate::domain::meeting::MeetingBundle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectedCalendarAccount {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub external_account_id: String,
    pub calendar_id: String,
    pub access_token_ciphertext: String,
    pub refresh_token_ciphertext: Option<String>,
    pub token_last_four: String,
    pub sync_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarEventSync {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub account_id: Uuid,
    pub provider: String,
    pub provider_event_id: String,
    pub provider_event_url: Option<String>,
    pub calendar_id: String,
    pub status: String,
    pub last_synced_at: String,
    pub failure_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarEventSyncResult {
    pub event: CalendarEventSync,
    pub operation: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectCalendarAccount {
    pub external_account_id: String,
    pub calendar_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEventUpsert {
    pub provider_event_id: String,
    pub provider_event_url: Option<String>,
}

#[derive(Debug)]
pub enum CalendarSyncError {
    InvalidInput(&'static str),
    AccountNotConnected(&'static str),
    StoreUnavailable,
    ProviderUnavailable,
}

impl CalendarSyncError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) | Self::AccountNotConnected(_) => StatusCode::BAD_REQUEST,
            Self::StoreUnavailable | Self::ProviderUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::AccountNotConnected(_) => "calendar_account_not_connected",
            Self::StoreUnavailable => "calendar_store_unavailable",
            Self::ProviderUnavailable => "calendar_provider_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::AccountNotConnected(message) => message,
            Self::StoreUnavailable => "calendar store is unavailable",
            Self::ProviderUnavailable => "calendar provider is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait CalendarStore: Send + Sync {
    async fn upsert_account(
        &self,
        account: ConnectedCalendarAccount,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError>;
    async fn list_accounts(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ConnectedCalendarAccount>, CalendarSyncError>;
    async fn connected_account(
        &self,
        user_id: Uuid,
        provider: String,
    ) -> Result<Option<ConnectedCalendarAccount>, CalendarSyncError>;
    async fn event_sync_for_meeting(
        &self,
        meeting_id: Uuid,
        account_id: Uuid,
        provider: String,
    ) -> Result<Option<CalendarEventSync>, CalendarSyncError>;
    async fn upsert_event_sync(
        &self,
        event: CalendarEventSync,
    ) -> Result<CalendarEventSync, CalendarSyncError>;
}

#[async_trait::async_trait]
pub trait CalendarProviderAdapter: Send + Sync {
    async fn upsert_event(
        &self,
        account: &ConnectedCalendarAccount,
        meeting: &MeetingBundle,
        public_url: &str,
        existing: Option<&CalendarEventSync>,
    ) -> Result<ProviderEventUpsert, CalendarSyncError>;
}

#[derive(Default)]
pub struct LocalGoogleCalendarAdapter;

#[async_trait::async_trait]
impl CalendarProviderAdapter for LocalGoogleCalendarAdapter {
    async fn upsert_event(
        &self,
        _account: &ConnectedCalendarAccount,
        meeting: &MeetingBundle,
        public_url: &str,
        existing: Option<&CalendarEventSync>,
    ) -> Result<ProviderEventUpsert, CalendarSyncError> {
        let provider_event_id = existing
            .map(|event| event.provider_event_id.clone())
            .unwrap_or_else(|| format!("google-{}", ids::new_uuid_v7().simple()));
        let _meeting_url = meeting_join_url(public_url, &meeting.meeting.join_slug);
        let provider_event_url = Some(format!(
            "https://calendar.google.com/calendar/event?eid={provider_event_id}"
        ));

        Ok(ProviderEventUpsert {
            provider_event_id,
            provider_event_url,
        })
    }
}

#[derive(Default)]
pub struct LocalMicrosoftCalendarAdapter;

#[async_trait::async_trait]
impl CalendarProviderAdapter for LocalMicrosoftCalendarAdapter {
    async fn upsert_event(
        &self,
        _account: &ConnectedCalendarAccount,
        meeting: &MeetingBundle,
        public_url: &str,
        existing: Option<&CalendarEventSync>,
    ) -> Result<ProviderEventUpsert, CalendarSyncError> {
        let provider_event_id = existing
            .map(|event| event.provider_event_id.clone())
            .unwrap_or_else(|| format!("microsoft-{}", ids::new_uuid_v7().simple()));
        let _meeting_url = meeting_join_url(public_url, &meeting.meeting.join_slug);
        let provider_event_url = Some(format!(
            "https://outlook.office.com/calendar/item/{provider_event_id}"
        ));

        Ok(ProviderEventUpsert {
            provider_event_id,
            provider_event_url,
        })
    }
}

#[derive(Clone)]
pub struct CalendarSyncService {
    store: std::sync::Arc<dyn CalendarStore>,
    google: std::sync::Arc<dyn CalendarProviderAdapter>,
    microsoft: std::sync::Arc<dyn CalendarProviderAdapter>,
}

impl CalendarSyncService {
    pub fn new(
        store: std::sync::Arc<dyn CalendarStore>,
        google: std::sync::Arc<dyn CalendarProviderAdapter>,
        microsoft: std::sync::Arc<dyn CalendarProviderAdapter>,
    ) -> Self {
        Self {
            store,
            google,
            microsoft,
        }
    }

    pub async fn connect_google_account(
        &self,
        user_id: Uuid,
        input: ConnectCalendarAccount,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
        self.connect_provider_account(
            user_id,
            input,
            "google",
            "google external_account_id is required",
            "google calendar_id is required",
        )
        .await
    }

    pub async fn connect_microsoft_account(
        &self,
        user_id: Uuid,
        input: ConnectCalendarAccount,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
        self.connect_provider_account(
            user_id,
            input,
            "microsoft",
            "microsoft external_account_id is required",
            "microsoft calendar_id is required",
        )
        .await
    }

    async fn connect_provider_account(
        &self,
        user_id: Uuid,
        input: ConnectCalendarAccount,
        provider: &'static str,
        external_account_message: &'static str,
        calendar_message: &'static str,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
        let now = now_string();
        let account = ConnectedCalendarAccount {
            id: ids::new_uuid_v7(),
            user_id,
            provider: provider.to_owned(),
            external_account_id: normalize_required(
                input.external_account_id,
                external_account_message,
                256,
            )?,
            calendar_id: normalize_calendar_id(input.calendar_id, calendar_message)?,
            token_last_four: token_last_four(&input.access_token)?,
            access_token_ciphertext: token_storage_value(input.access_token)?,
            refresh_token_ciphertext: input.refresh_token.map(token_storage_value).transpose()?,
            sync_enabled: true,
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.upsert_account(account).await
    }

    pub async fn list_accounts(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ConnectedCalendarAccount>, CalendarSyncError> {
        self.store.list_accounts(user_id).await
    }

    pub async fn sync_google_meeting(
        &self,
        user_id: Uuid,
        meeting: MeetingBundle,
        public_url: &str,
    ) -> Result<CalendarEventSyncResult, CalendarSyncError> {
        self.sync_provider_meeting(
            user_id,
            meeting,
            public_url,
            "google",
            self.google.as_ref(),
            "google calendar account is not connected",
        )
        .await
    }

    pub async fn sync_microsoft_meeting(
        &self,
        user_id: Uuid,
        meeting: MeetingBundle,
        public_url: &str,
    ) -> Result<CalendarEventSyncResult, CalendarSyncError> {
        self.sync_provider_meeting(
            user_id,
            meeting,
            public_url,
            "microsoft",
            self.microsoft.as_ref(),
            "microsoft calendar account is not connected",
        )
        .await
    }

    async fn sync_provider_meeting(
        &self,
        user_id: Uuid,
        meeting: MeetingBundle,
        public_url: &str,
        provider: &'static str,
        adapter: &dyn CalendarProviderAdapter,
        not_connected_message: &'static str,
    ) -> Result<CalendarEventSyncResult, CalendarSyncError> {
        let account = self
            .store
            .connected_account(user_id, provider.to_owned())
            .await?
            .filter(|account| account.sync_enabled)
            .ok_or(CalendarSyncError::AccountNotConnected(
                not_connected_message,
            ))?;
        let existing = self
            .store
            .event_sync_for_meeting(meeting.meeting.id, account.id, provider.to_owned())
            .await?;
        let operation = if existing.is_some() {
            "updated"
        } else {
            "created"
        };
        let provider_event = adapter
            .upsert_event(&account, &meeting, public_url, existing.as_ref())
            .await?;
        let now = now_string();
        let event = CalendarEventSync {
            id: existing
                .as_ref()
                .map(|event| event.id)
                .unwrap_or_else(ids::new_uuid_v7),
            meeting_id: meeting.meeting.id,
            account_id: account.id,
            provider: provider.to_owned(),
            provider_event_id: provider_event.provider_event_id,
            provider_event_url: provider_event.provider_event_url,
            calendar_id: account.calendar_id,
            status: "synced".to_owned(),
            last_synced_at: now.clone(),
            failure_reason: None,
            created_at: existing
                .as_ref()
                .map(|event| event.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };

        let event = self.store.upsert_event_sync(event).await?;
        Ok(CalendarEventSyncResult {
            event,
            operation: operation.to_owned(),
        })
    }
}

fn normalize_calendar_id(
    value: String,
    message: &'static str,
) -> Result<String, CalendarSyncError> {
    let value = value.trim();
    if value.is_empty() {
        Ok("primary".to_owned())
    } else {
        normalize_required(value.to_owned(), message, 256)
    }
}

fn normalize_required(
    value: String,
    message: &'static str,
    max_len: usize,
) -> Result<String, CalendarSyncError> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=max_len).contains(&value.len()) {
        Ok(value)
    } else {
        Err(CalendarSyncError::InvalidInput(message))
    }
}

fn token_last_four(token: &str) -> Result<String, CalendarSyncError> {
    let token = token.trim();
    if token.len() < 8 {
        return Err(CalendarSyncError::InvalidInput(
            "google access_token must be at least 8 characters",
        ));
    }

    Ok(token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

fn token_storage_value(token: String) -> Result<String, CalendarSyncError> {
    let token = token.trim();
    if token.len() < 8 {
        return Err(CalendarSyncError::InvalidInput(
            "google access_token must be at least 8 characters",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn meeting_join_url(public_url: &str, join_slug: &str) -> String {
    let public_url = public_url.trim_end_matches('/');
    if public_url.is_empty() {
        format!("/join/{join_slug}")
    } else {
        format!("{public_url}/join/{join_slug}")
    }
}
