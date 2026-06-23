use serde::{Deserialize, Serialize};

use crate::domain::space::SpaceMembership;

#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchSpaceRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SpaceResponse {
    pub id: String,
    pub organization_id: String,
    pub slug: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct SpaceMemberResponse {
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct SpaceMembershipResponse {
    pub space: SpaceResponse,
    pub membership: SpaceMemberResponse,
}

#[derive(Debug, Serialize)]
pub struct SpaceListResponse {
    pub spaces: Vec<SpaceResponse>,
}

impl From<SpaceMembership> for SpaceResponse {
    fn from(membership: SpaceMembership) -> Self {
        Self {
            id: membership.id.to_string(),
            organization_id: membership.organization_id.to_string(),
            slug: membership.slug,
            name: membership.name,
            role: membership.role,
        }
    }
}

impl From<PatchSpaceRequest> for crate::domain::space::SpacePatch {
    fn from(request: PatchSpaceRequest) -> Self {
        Self { name: request.name }
    }
}
