use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::scim::{ScimError, ScimStore, StoredScimToken, StoredScimUser};

#[derive(Default)]
pub struct MemoryScimStore {
    state: Mutex<MemoryScimState>,
}

#[derive(Default)]
struct MemoryScimState {
    tokens_by_hash: HashMap<String, StoredScimToken>,
    token_hash_by_organization: HashMap<Uuid, String>,
    users_by_organization_external_id: HashMap<(Uuid, String), StoredScimUser>,
}

#[async_trait::async_trait]
impl ScimStore for MemoryScimStore {
    async fn rotate_token(&self, token: StoredScimToken) -> Result<(), ScimError> {
        let mut state = self.state.lock().map_err(|_| ScimError::StoreUnavailable)?;
        if let Some(previous_hash) = state
            .token_hash_by_organization
            .insert(token.organization_id, token.token_hash.clone())
        {
            state.tokens_by_hash.remove(&previous_hash);
        }
        state.tokens_by_hash.insert(token.token_hash.clone(), token);
        Ok(())
    }

    async fn token_by_hash(&self, token_hash: &str) -> Result<Option<StoredScimToken>, ScimError> {
        let state = self.state.lock().map_err(|_| ScimError::StoreUnavailable)?;
        Ok(state.tokens_by_hash.get(token_hash).cloned())
    }

    async fn upsert_user(&self, user: StoredScimUser) -> Result<StoredScimUser, ScimError> {
        let mut state = self.state.lock().map_err(|_| ScimError::StoreUnavailable)?;
        let key = (user.organization_id, user.external_id.clone());
        let user = if let Some(existing) = state.users_by_organization_external_id.get_mut(&key) {
            existing.user_id = user.user_id;
            existing.user_name = user.user_name;
            existing.display_name = user.display_name;
            existing.active = user.active;
            existing.clone()
        } else {
            state
                .users_by_organization_external_id
                .insert(key, user.clone());
            user
        };

        Ok(user)
    }

    async fn user_by_external_id(
        &self,
        organization_id: Uuid,
        external_id: &str,
    ) -> Result<Option<StoredScimUser>, ScimError> {
        let state = self.state.lock().map_err(|_| ScimError::StoreUnavailable)?;
        Ok(state
            .users_by_organization_external_id
            .get(&(organization_id, external_id.to_owned()))
            .cloned())
    }

    async fn set_user_active(
        &self,
        organization_id: Uuid,
        external_id: &str,
        active: bool,
    ) -> Result<StoredScimUser, ScimError> {
        let mut state = self.state.lock().map_err(|_| ScimError::StoreUnavailable)?;
        let Some(user) = state
            .users_by_organization_external_id
            .get_mut(&(organization_id, external_id.to_owned()))
        else {
            return Err(ScimError::NotFound);
        };
        user.active = active;
        Ok(user.clone())
    }
}
