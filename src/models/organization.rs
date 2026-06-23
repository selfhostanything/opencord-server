use serde::{Deserialize, Serialize};

use crate::domain::organization::{
    CustomDomain, CustomDomainTenant, OrganizationMembership, TenantProvision,
};

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

#[derive(Debug, Deserialize)]
pub struct CreateCustomDomainRequest {
    pub hostname: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyCustomDomainRequest {
    pub verification_token: String,
}

#[derive(Debug, Serialize)]
pub struct CustomDomainResponse {
    pub id: String,
    pub organization_id: String,
    pub hostname: String,
    pub verification_token: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CustomDomainEnvelope {
    pub custom_domain: CustomDomainResponse,
}

#[derive(Debug, Serialize)]
pub struct CustomDomainListResponse {
    pub custom_domains: Vec<CustomDomainResponse>,
}

#[derive(Debug, Serialize)]
pub struct CustomDomainTenantResponse {
    pub organization_id: String,
    pub slug: String,
    pub name: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
    pub hostname: String,
}

#[derive(Debug, Serialize)]
pub struct CustomDomainResolveResponse {
    pub tenant: CustomDomainTenantResponse,
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

impl From<CustomDomain> for CustomDomainResponse {
    fn from(custom_domain: CustomDomain) -> Self {
        Self {
            id: custom_domain.id.to_string(),
            organization_id: custom_domain.organization_id.to_string(),
            hostname: custom_domain.hostname,
            verification_token: custom_domain.verification_token,
            status: custom_domain.status,
        }
    }
}

impl From<CustomDomainTenant> for CustomDomainResolveResponse {
    fn from(tenant: CustomDomainTenant) -> Self {
        Self {
            tenant: CustomDomainTenantResponse {
                organization_id: tenant.organization_id.to_string(),
                slug: tenant.slug,
                name: tenant.name,
                plan: tenant.plan,
                deployment_mode: tenant.deployment_mode,
                primary_region: tenant.primary_region,
                hostname: tenant.hostname,
            },
        }
    }
}
