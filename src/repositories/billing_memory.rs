use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::billing::{BillingError, BillingState, BillingStore};

#[derive(Default)]
pub struct MemoryBillingStore {
    states_by_organization_id: Mutex<HashMap<Uuid, BillingState>>,
}

#[async_trait::async_trait]
impl BillingStore for MemoryBillingStore {
    async fn upsert_state(&self, mut state: BillingState) -> Result<BillingState, BillingError> {
        let mut states = self
            .states_by_organization_id
            .lock()
            .map_err(|_| BillingError::StoreUnavailable)?;
        if let Some(existing) = states.get(&state.organization_id) {
            state.id = existing.id;
        }
        states.insert(state.organization_id, state.clone());

        Ok(state)
    }
}
