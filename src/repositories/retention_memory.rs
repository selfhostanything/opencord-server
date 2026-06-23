use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::retention::{RetentionError, RetentionPolicy, RetentionRun, RetentionStore};

#[derive(Default)]
pub struct MemoryRetentionStore {
    state: Mutex<MemoryRetentionState>,
}

#[derive(Default)]
struct MemoryRetentionState {
    policies_by_organization_id: HashMap<Uuid, RetentionPolicy>,
    runs_by_organization_id: HashMap<Uuid, Vec<RetentionRun>>,
}

#[async_trait::async_trait]
impl RetentionStore for MemoryRetentionStore {
    async fn upsert_policy(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RetentionError::StoreUnavailable)?;
        state
            .policies_by_organization_id
            .insert(policy.organization_id, policy.clone());
        Ok(policy)
    }

    async fn get_policy(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| RetentionError::StoreUnavailable)?;
        Ok(state
            .policies_by_organization_id
            .get(&organization_id)
            .cloned())
    }

    async fn list_policies(&self) -> Result<Vec<RetentionPolicy>, RetentionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| RetentionError::StoreUnavailable)?;
        let mut policies = state
            .policies_by_organization_id
            .values()
            .cloned()
            .collect::<Vec<_>>();
        policies.sort_by_key(|policy| policy.organization_id);
        Ok(policies)
    }

    async fn record_run(&self, run: RetentionRun) -> Result<(), RetentionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RetentionError::StoreUnavailable)?;
        state
            .runs_by_organization_id
            .entry(run.organization_id)
            .or_default()
            .push(run);
        Ok(())
    }

    async fn list_runs_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<RetentionRun>, RetentionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| RetentionError::StoreUnavailable)?;
        let mut runs = state
            .runs_by_organization_id
            .get(&organization_id)
            .cloned()
            .unwrap_or_default();
        runs.sort_by(|left, right| {
            left.ran_at
                .cmp(&right.ran_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(runs)
    }
}
