use serde::{Deserialize, Serialize};

use crate::domain::organization::OrganizationMembership;

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

impl From<OrganizationMembership> for OrganizationResponse {
    fn from(membership: OrganizationMembership) -> Self {
        Self {
            id: membership.id.to_string(),
            slug: membership.slug,
            name: membership.name,
            role: membership.role,
        }
    }
}
