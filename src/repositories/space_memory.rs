use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::space::{
    SpaceError, SpaceMembership, SpaceStore, StoredSpace, StoredSpaceMember,
};

#[derive(Default)]
pub struct MemorySpaceStore {
    state: Mutex<MemorySpaceState>,
}

#[derive(Default)]
struct MemorySpaceState {
    spaces_by_id: HashMap<Uuid, StoredSpace>,
    space_id_by_org_slug: HashMap<(Uuid, String), Uuid>,
    members_by_space_user: HashMap<(Uuid, Uuid), StoredSpaceMember>,
}

#[async_trait::async_trait]
impl SpaceStore for MemorySpaceStore {
    async fn create_space(
        &self,
        space: StoredSpace,
        owner_member: StoredSpaceMember,
    ) -> Result<(), SpaceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;
        let key = (space.organization_id, space.slug.clone());

        if state.space_id_by_org_slug.contains_key(&key) {
            return Err(SpaceError::SlugAlreadyExists);
        }

        state.space_id_by_org_slug.insert(key, space.id);
        state
            .members_by_space_user
            .insert((owner_member.space_id, owner_member.user_id), owner_member);
        state.spaces_by_id.insert(space.id, space);

        Ok(())
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Vec<SpaceMembership>, SpaceError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;

        let mut spaces = state
            .members_by_space_user
            .values()
            .filter(|member| member.user_id == user_id && member.status == "active")
            .filter_map(|member| {
                state.spaces_by_id.get(&member.space_id).and_then(|space| {
                    (space.organization_id == organization_id).then(|| SpaceMembership {
                        id: space.id,
                        organization_id: space.organization_id,
                        slug: space.slug.clone(),
                        name: space.name.clone(),
                        role: member.role.clone(),
                    })
                })
            })
            .collect::<Vec<_>>();

        spaces.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(spaces)
    }
}
