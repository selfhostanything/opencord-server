use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::organization::{
    OrganizationError, OrganizationMembership, OrganizationStore, StoredOrganization,
    StoredOrganizationMember,
};

#[derive(Default)]
pub struct MemoryOrganizationStore {
    state: Mutex<MemoryOrganizationState>,
}

#[derive(Default)]
struct MemoryOrganizationState {
    organizations_by_id: HashMap<Uuid, StoredOrganization>,
    organization_id_by_slug: HashMap<String, Uuid>,
    members_by_org_user: HashMap<(Uuid, Uuid), StoredOrganizationMember>,
}

#[async_trait::async_trait]
impl OrganizationStore for MemoryOrganizationStore {
    async fn create_organization(
        &self,
        organization: StoredOrganization,
        owner_member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        if state
            .organization_id_by_slug
            .contains_key(&organization.slug)
        {
            return Err(OrganizationError::SlugAlreadyExists);
        }

        state
            .organization_id_by_slug
            .insert(organization.slug.clone(), organization.id);
        state.members_by_org_user.insert(
            (owner_member.organization_id, owner_member.user_id),
            owner_member,
        );
        state
            .organizations_by_id
            .insert(organization.id, organization);

        Ok(())
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        let mut organizations = state
            .members_by_org_user
            .values()
            .filter(|member| member.user_id == user_id && member.status == "active")
            .filter_map(|member| {
                state
                    .organizations_by_id
                    .get(&member.organization_id)
                    .map(|organization| OrganizationMembership {
                        id: organization.id,
                        slug: organization.slug.clone(),
                        name: organization.name.clone(),
                        role: member.role.clone(),
                    })
            })
            .collect::<Vec<_>>();

        organizations.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(organizations)
    }

    async fn get_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Option<OrganizationMembership>, OrganizationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        let Some(member) = state.members_by_org_user.get(&(organization_id, user_id)) else {
            return Ok(None);
        };

        if member.status != "active" {
            return Ok(None);
        }

        Ok(state
            .organizations_by_id
            .get(&organization_id)
            .map(|organization| OrganizationMembership {
                id: organization.id,
                slug: organization.slug.clone(),
                name: organization.name.clone(),
                role: member.role.clone(),
            }))
    }
}
