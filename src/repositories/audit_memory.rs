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
}
