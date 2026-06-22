use serde::{Deserialize, Serialize};

use crate::domain::permission::{
    ChannelPermissionOverride, PermissionTargetKind, Role, RoleAssignment, permission_names,
};

#[derive(Debug, Deserialize)]
pub struct AddSpaceMemberRequest {
    pub user_id: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SpaceMemberDetailResponse {
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SpaceMemberResourceResponse {
    pub member: SpaceMemberDetailResponse,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub color: Option<String>,
    pub position: Option<i32>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub name: String,
    pub color: Option<String>,
    pub position: i32,
    pub permissions: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct RoleResourceResponse {
    pub role: RoleResponse,
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct RoleAssignmentResponse {
    pub space_id: String,
    pub role_id: String,
    pub user_id: String,
    pub assigned_by_user_id: String,
}

#[derive(Debug, Serialize)]
pub struct RoleAssignmentResourceResponse {
    pub assignment: RoleAssignmentResponse,
}

#[derive(Debug, Deserialize)]
pub struct SetChannelPermissionOverrideRequest {
    pub target_kind: String,
    pub target_id: String,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ChannelPermissionOverrideResponse {
    pub channel_id: String,
    pub target_kind: &'static str,
    pub target_id: String,
    pub allow: Vec<&'static str>,
    pub deny: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ChannelPermissionOverrideResourceResponse {
    pub permission_override: ChannelPermissionOverrideResponse,
}

impl From<Role> for RoleResponse {
    fn from(role: Role) -> Self {
        Self {
            id: role.id.to_string(),
            organization_id: role.organization_id.to_string(),
            space_id: role.space_id.to_string(),
            name: role.name,
            color: role.color,
            position: role.position,
            permissions: permission_names(role.permissions_bitset),
        }
    }
}

impl From<RoleAssignment> for RoleAssignmentResponse {
    fn from(assignment: RoleAssignment) -> Self {
        Self {
            space_id: assignment.space_id.to_string(),
            role_id: assignment.role_id.to_string(),
            user_id: assignment.user_id.to_string(),
            assigned_by_user_id: assignment.assigned_by_user_id.to_string(),
        }
    }
}

impl From<ChannelPermissionOverride> for ChannelPermissionOverrideResponse {
    fn from(permission_override: ChannelPermissionOverride) -> Self {
        let target_kind = match permission_override.target_kind {
            PermissionTargetKind::Role => PermissionTargetKind::Role.as_str(),
            PermissionTargetKind::Member => PermissionTargetKind::Member.as_str(),
        };

        Self {
            channel_id: permission_override.channel_id.to_string(),
            target_kind,
            target_id: permission_override.target_id.to_string(),
            allow: permission_names(permission_override.allow_bitset),
            deny: permission_names(permission_override.deny_bitset),
        }
    }
}
