use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::audit::{AuditError, AuditEvent, AuditStore};

#[derive(Default)]
pub struct MemoryAuditStore {
    state: Mutex<MemoryAuditState>,
}

#[derive(Default)]
struct MemoryAuditState {
    events_by_space_id: HashMap<Uuid, Vec<AuditEvent>>,
}

#[async_trait::async_trait]
impl AuditStore for MemoryAuditStore {
    async fn create_event(&self, event: AuditEvent) -> Result<(), AuditError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AuditError::StoreUnavailable)?;
        state
            .events_by_space_id
            .entry(event.space_id)
            .or_default()
            .push(event);
        Ok(())
    }

    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AuditError::StoreUnavailable)?;
        let mut events = state
            .events_by_space_id
            .get(&space_id)
            .cloned()
            .unwrap_or_default();
        events.sort_by_key(|event| event.id);
        Ok(events)
    }

    async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AuditError::StoreUnavailable)?;
        let mut events = state
            .events_by_space_id
            .values()
            .flat_map(|events| events.iter())
            .filter(|event| {
                event.organization_id == organization_id
                    && event.created_at.as_str() >= from.as_str()
                    && event.created_at.as_str() <= to.as_str()
            })
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(events)
    }

    async fn purge_for_retention(
        &self,
        organization_id: Uuid,
        created_before: Option<String>,
        dry_run: bool,
    ) -> Result<usize, AuditError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AuditError::StoreUnavailable)?;
        let mut purged = 0;

        for events in state.events_by_space_id.values_mut() {
            let expired_count = events
                .iter()
                .filter(|event| {
                    event.organization_id == organization_id
                        && created_before
                            .as_deref()
                            .is_some_and(|cutoff| event.created_at.as_str() < cutoff)
                })
                .count();
            if !dry_run {
                events.retain(|event| {
                    event.organization_id != organization_id
                        || created_before
                            .as_deref()
                            .is_none_or(|cutoff| event.created_at.as_str() >= cutoff)
                });
            }
            purged += expired_count;
        }

        Ok(purged)
    }
}
