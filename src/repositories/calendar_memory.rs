use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::calendar_sync::{
    CalendarEventSync, CalendarStore, CalendarSyncError, ConnectedCalendarAccount,
};

#[derive(Default)]
pub struct MemoryCalendarStore {
    state: Mutex<MemoryCalendarState>,
}

#[derive(Default)]
struct MemoryCalendarState {
    accounts_by_user_provider: HashMap<(Uuid, String), ConnectedCalendarAccount>,
    event_syncs_by_meeting_account_provider: HashMap<(Uuid, Uuid, String), CalendarEventSync>,
}

#[async_trait::async_trait]
impl CalendarStore for MemoryCalendarStore {
    async fn upsert_account(
        &self,
        mut account: ConnectedCalendarAccount,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        let key = (account.user_id, account.provider.clone());
        if let Some(existing) = state.accounts_by_user_provider.get(&key) {
            account.id = existing.id;
            account.created_at = existing.created_at.clone();
        }

        state.accounts_by_user_provider.insert(key, account.clone());
        Ok(account)
    }

    async fn list_accounts(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ConnectedCalendarAccount>, CalendarSyncError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        let mut accounts = state
            .accounts_by_user_provider
            .values()
            .filter(|account| account.user_id == user_id)
            .cloned()
            .collect::<Vec<_>>();
        accounts.sort_by(|left, right| left.provider.cmp(&right.provider));

        Ok(accounts)
    }

    async fn connected_account(
        &self,
        user_id: Uuid,
        provider: String,
    ) -> Result<Option<ConnectedCalendarAccount>, CalendarSyncError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        Ok(state
            .accounts_by_user_provider
            .get(&(user_id, provider))
            .cloned())
    }

    async fn event_sync_for_meeting(
        &self,
        meeting_id: Uuid,
        account_id: Uuid,
        provider: String,
    ) -> Result<Option<CalendarEventSync>, CalendarSyncError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        Ok(state
            .event_syncs_by_meeting_account_provider
            .get(&(meeting_id, account_id, provider))
            .cloned())
    }

    async fn upsert_event_sync(
        &self,
        event: CalendarEventSync,
    ) -> Result<CalendarEventSync, CalendarSyncError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        state.event_syncs_by_meeting_account_provider.insert(
            (event.meeting_id, event.account_id, event.provider.clone()),
            event.clone(),
        );

        Ok(event)
    }

    async fn count_accounts_for_user_ids(
        &self,
        user_ids: &[Uuid],
    ) -> Result<i64, CalendarSyncError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;
        let user_ids = user_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();

        Ok(state
            .accounts_by_user_provider
            .values()
            .filter(|account| account.sync_enabled && user_ids.contains(&account.user_id))
            .count() as i64)
    }
}
