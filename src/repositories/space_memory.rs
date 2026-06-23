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

    async fn get_for_user(
        &self,
        user_id: Uuid,
        space_id: Uuid,
    ) -> Result<Option<SpaceMembership>, SpaceError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;
        let Some(member) = state.members_by_space_user.get(&(space_id, user_id)) else {
            return Ok(None);
        };

        if member.status != "active" {
            return Ok(None);
        }

        Ok(state
            .spaces_by_id
            .get(&space_id)
            .map(|space| SpaceMembership {
                id: space.id,
                organization_id: space.organization_id,
                slug: space.slug.clone(),
                name: space.name.clone(),
                role: member.role.clone(),
            }))
    }

    async fn add_member(&self, member: StoredSpaceMember) -> Result<SpaceMembership, SpaceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;
        let Some(space) = state.spaces_by_id.get(&member.space_id).cloned() else {
            return Err(SpaceError::NotFound);
        };
        let key = (member.space_id, member.user_id);

        state
            .members_by_space_user
            .entry(key)
            .and_modify(|existing| {
                if existing.role != "owner" {
                    existing.role = member.role.clone();
                }
                existing.status = "active".to_owned();
            })
            .or_insert(member.clone());

        let stored_member = state
            .members_by_space_user
            .get(&(member.space_id, member.user_id))
            .ok_or(SpaceError::StoreUnavailable)?;

        Ok(SpaceMembership {
            id: space.id,
            organization_id: space.organization_id,
            slug: space.slug,
            name: space.name,
            role: stored_member.role.clone(),
        })
    }

    async fn remove_member(&self, space_id: Uuid, user_id: Uuid) -> Result<(), SpaceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;
        let Some(member) = state.members_by_space_user.get_mut(&(space_id, user_id)) else {
            return Err(SpaceError::NotFound);
        };
        if member.status != "active" {
            return Err(SpaceError::NotFound);
        }

        member.status = "inactive".to_owned();
        Ok(())
    }

    async fn update_space(
        &self,
        membership: SpaceMembership,
    ) -> Result<SpaceMembership, SpaceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SpaceError::StoreUnavailable)?;
        let Some(previous) = state.spaces_by_id.get(&membership.id).cloned() else {
            return Err(SpaceError::NotFound);
        };
        let next_key = (membership.organization_id, membership.slug.clone());

        if state
            .space_id_by_org_slug
            .get(&next_key)
            .is_some_and(|existing_id| *existing_id != membership.id)
        {
            return Err(SpaceError::SlugAlreadyExists);
        }

        state
            .space_id_by_org_slug
            .remove(&(previous.organization_id, previous.slug.clone()));
        state.space_id_by_org_slug.insert(next_key, membership.id);
        state.spaces_by_id.insert(
            membership.id,
            StoredSpace {
                id: membership.id,
                organization_id: membership.organization_id,
                slug: membership.slug.clone(),
                name: membership.name.clone(),
                owner_user_id: previous.owner_user_id,
            },
        );

        Ok(membership)
    }
}
