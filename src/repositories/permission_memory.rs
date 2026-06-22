use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::permission::{
    AssignedRole, ChannelPermissionOverride, PermissionError, PermissionStore, Role, RoleAssignment,
};

#[derive(Default)]
pub struct MemoryPermissionStore {
    state: Mutex<MemoryPermissionState>,
}

#[derive(Default)]
struct MemoryPermissionState {
    roles_by_id: HashMap<Uuid, Role>,
    role_id_by_space_name: HashMap<(Uuid, String), Uuid>,
    assignments_by_role_user: HashMap<(Uuid, Uuid), RoleAssignment>,
    overrides_by_target: HashMap<(Uuid, &'static str, Uuid), ChannelPermissionOverride>,
}

#[async_trait::async_trait]
impl PermissionStore for MemoryPermissionStore {
    async fn create_role(&self, role: Role) -> Result<(), PermissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;
        let key = (role.space_id, role.name.to_ascii_lowercase());

        if state.role_id_by_space_name.contains_key(&key) {
            return Err(PermissionError::RoleAlreadyExists);
        }

        state.role_id_by_space_name.insert(key, role.id);
        state.roles_by_id.insert(role.id, role);

        Ok(())
    }

    async fn get_role(
        &self,
        space_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<Role>, PermissionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;

        Ok(state
            .roles_by_id
            .get(&role_id)
            .filter(|role| role.space_id == space_id)
            .cloned())
    }

    async fn assign_role(&self, assignment: RoleAssignment) -> Result<(), PermissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;

        if !state.roles_by_id.contains_key(&assignment.role_id) {
            return Err(PermissionError::RoleNotFound);
        }

        state
            .assignments_by_role_user
            .insert((assignment.role_id, assignment.user_id), assignment);

        Ok(())
    }

    async fn assigned_roles_for_user(
        &self,
        space_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<AssignedRole>, PermissionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;

        Ok(state
            .assignments_by_role_user
            .values()
            .filter(|assignment| assignment.space_id == space_id && assignment.user_id == user_id)
            .filter_map(|assignment| {
                state
                    .roles_by_id
                    .get(&assignment.role_id)
                    .map(|role| AssignedRole {
                        id: role.id,
                        permissions_bitset: role.permissions_bitset,
                    })
            })
            .collect())
    }

    async fn upsert_channel_override(
        &self,
        permission_override: ChannelPermissionOverride,
    ) -> Result<(), PermissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;
        let key = (
            permission_override.channel_id,
            permission_override.target_kind.as_str(),
            permission_override.target_id,
        );

        state.overrides_by_target.insert(key, permission_override);

        Ok(())
    }

    async fn channel_overrides_for_user(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        role_ids: &[Uuid],
    ) -> Result<Vec<ChannelPermissionOverride>, PermissionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PermissionError::StoreUnavailable)?;

        Ok(state
            .overrides_by_target
            .values()
            .filter(|permission_override| permission_override.channel_id == channel_id)
            .filter(|permission_override| {
                permission_override.target_id == user_id
                    || role_ids.contains(&permission_override.target_id)
            })
            .cloned()
            .collect())
    }
}
