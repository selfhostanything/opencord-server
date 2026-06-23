use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::organization::{
    CustomDomain, CustomDomainTenant, OrganizationError, OrganizationMembership, OrganizationStore,
    StoredCustomDomain, StoredOrganization, StoredOrganizationMember,
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
    custom_domains_by_id: HashMap<Uuid, StoredCustomDomain>,
    custom_domain_id_by_hostname: HashMap<String, Uuid>,
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
                        plan: organization.plan.clone(),
                        deployment_mode: organization.deployment_mode.clone(),
                        primary_region: organization.primary_region.clone(),
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
                plan: organization.plan.clone(),
                deployment_mode: organization.deployment_mode.clone(),
                primary_region: organization.primary_region.clone(),
            }))
    }

    async fn active_member_user_ids(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Uuid>, OrganizationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;
        if !state.organizations_by_id.contains_key(&organization_id) {
            return Err(OrganizationError::NotFound);
        }

        let mut user_ids = state
            .members_by_org_user
            .values()
            .filter(|member| member.organization_id == organization_id && member.status == "active")
            .map(|member| member.user_id)
            .collect::<Vec<_>>();
        user_ids.sort();

        Ok(user_ids)
    }

    async fn update_plan(
        &self,
        organization_id: Uuid,
        plan: String,
    ) -> Result<(), OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;
        let Some(organization) = state.organizations_by_id.get_mut(&organization_id) else {
            return Err(OrganizationError::NotFound);
        };

        organization.plan = plan;
        Ok(())
    }

    async fn add_member_if_missing(
        &self,
        member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        if !state
            .organizations_by_id
            .contains_key(&member.organization_id)
        {
            return Err(OrganizationError::NotFound);
        }

        state
            .members_by_org_user
            .entry((member.organization_id, member.user_id))
            .and_modify(|existing| {
                existing.status = "active".to_owned();
            })
            .or_insert(member);

        Ok(())
    }

    async fn set_member_status(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
        status: String,
    ) -> Result<(), OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;
        let Some(member) = state
            .members_by_org_user
            .get_mut(&(organization_id, user_id))
        else {
            return Err(OrganizationError::NotFound);
        };

        member.status = status;
        Ok(())
    }

    async fn create_custom_domain(
        &self,
        custom_domain: StoredCustomDomain,
    ) -> Result<(), OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        if !state
            .organizations_by_id
            .contains_key(&custom_domain.organization_id)
        {
            return Err(OrganizationError::NotFound);
        }

        if state
            .custom_domain_id_by_hostname
            .contains_key(&custom_domain.hostname)
        {
            return Err(OrganizationError::CustomDomainAlreadyExists);
        }

        state
            .custom_domain_id_by_hostname
            .insert(custom_domain.hostname.clone(), custom_domain.id);
        state
            .custom_domains_by_id
            .insert(custom_domain.id, custom_domain);

        Ok(())
    }

    async fn list_custom_domains(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<CustomDomain>, OrganizationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        if !state.organizations_by_id.contains_key(&organization_id) {
            return Err(OrganizationError::NotFound);
        }

        let mut domains = state
            .custom_domains_by_id
            .values()
            .filter(|custom_domain| custom_domain.organization_id == organization_id)
            .cloned()
            .map(CustomDomain::from)
            .collect::<Vec<_>>();
        domains.sort_by(|left, right| left.hostname.cmp(&right.hostname));

        Ok(domains)
    }

    async fn verify_custom_domain(
        &self,
        organization_id: Uuid,
        custom_domain_id: Uuid,
        verification_token: String,
    ) -> Result<CustomDomain, OrganizationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        let Some(custom_domain) = state.custom_domains_by_id.get_mut(&custom_domain_id) else {
            return Err(OrganizationError::NotFound);
        };

        if custom_domain.organization_id != organization_id {
            return Err(OrganizationError::NotFound);
        }

        if custom_domain.verification_token != verification_token {
            return Err(OrganizationError::InvalidInput(
                "custom domain verification token is invalid",
            ));
        }

        custom_domain.status = "active".to_owned();

        Ok(CustomDomain::from(custom_domain.clone()))
    }

    async fn resolve_custom_domain(
        &self,
        hostname: String,
    ) -> Result<Option<CustomDomainTenant>, OrganizationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        let Some(custom_domain_id) = state.custom_domain_id_by_hostname.get(&hostname) else {
            return Ok(None);
        };
        let Some(custom_domain) = state.custom_domains_by_id.get(custom_domain_id) else {
            return Ok(None);
        };

        if custom_domain.status != "active" {
            return Ok(None);
        }

        Ok(state
            .organizations_by_id
            .get(&custom_domain.organization_id)
            .map(|organization| CustomDomainTenant {
                organization_id: organization.id,
                slug: organization.slug.clone(),
                name: organization.name.clone(),
                plan: organization.plan.clone(),
                deployment_mode: organization.deployment_mode.clone(),
                primary_region: organization.primary_region.clone(),
                hostname: custom_domain.hostname.clone(),
            }))
    }
}
