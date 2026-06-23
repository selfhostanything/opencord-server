use serde::{Deserialize, Serialize};

use crate::domain::organization::{OrganizationMembership, TenantProvision};

#[derive(Debug, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct OrganizationResponse {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub role: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
}

#[derive(Debug, Serialize)]
pub struct MembershipResponse {
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct OrganizationMembershipResponse {
    pub organization: OrganizationResponse,
    pub membership: MembershipResponse,
}

#[derive(Debug, Serialize)]
pub struct OrganizationListResponse {
    pub organizations: Vec<OrganizationResponse>,
}

#[derive(Debug, Deserialize)]
pub struct ProvisionTenantRequest {
    pub name: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub organization_id: String,
    pub owner_user_id: String,
    pub slug: String,
    pub name: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
}

#[derive(Debug, Serialize)]
pub struct TenantProvisionResponse {
    pub tenant: TenantResponse,
}

impl From<OrganizationMembership> for OrganizationResponse {
    fn from(membership: OrganizationMembership) -> Self {
        Self {
            id: membership.id.to_string(),
            slug: membership.slug,
            name: membership.name,
            role: membership.role,
            plan: membership.plan,
            deployment_mode: membership.deployment_mode,
            primary_region: membership.primary_region,
        }
    }
}

impl From<TenantProvision> for TenantProvisionResponse {
    fn from(tenant: TenantProvision) -> Self {
        Self {
            tenant: TenantResponse {
                organization_id: tenant.organization.id.to_string(),
                owner_user_id: tenant.owner_user_id.to_string(),
                slug: tenant.organization.slug,
                name: tenant.organization.name,
                plan: tenant.organization.plan,
                deployment_mode: tenant.organization.deployment_mode,
                primary_region: tenant.organization.primary_region,
            },
        }
    }
}
