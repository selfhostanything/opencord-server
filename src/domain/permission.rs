use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::channel::Channel;
use crate::domain::ids;
use crate::domain::space::SpaceMembership;

pub const VIEW_CHANNEL: u64 = 1 << 0;
pub const SEND_MESSAGES: u64 = 1 << 1;
pub const MANAGE_MESSAGES: u64 = 1 << 2;
pub const MANAGE_CHANNELS: u64 = 1 << 3;
pub const MANAGE_ROLES: u64 = 1 << 4;
pub const INVITE_MEMBERS: u64 = 1 << 5;
pub const MANAGE_SPACE: u64 = 1 << 6;

const ALL_MINIMAL_PERMISSIONS: u64 = VIEW_CHANNEL
    | SEND_MESSAGES
    | MANAGE_MESSAGES
    | MANAGE_CHANNELS
    | MANAGE_ROLES
    | INVITE_MEMBERS
    | MANAGE_SPACE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Permission {
    ViewChannel,
    SendMessages,
    ManageMessages,
    ManageChannels,
    ManageRoles,
    InviteMembers,
    ManageSpace,
}

impl Permission {
    pub fn bit(self) -> u64 {
        match self {
            Self::ViewChannel => VIEW_CHANNEL,
            Self::SendMessages => SEND_MESSAGES,
            Self::ManageMessages => MANAGE_MESSAGES,
            Self::ManageChannels => MANAGE_CHANNELS,
            Self::ManageRoles => MANAGE_ROLES,
            Self::InviteMembers => INVITE_MEMBERS,
            Self::ManageSpace => MANAGE_SPACE,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ViewChannel => "VIEW_CHANNEL",
            Self::SendMessages => "SEND_MESSAGES",
            Self::ManageMessages => "MANAGE_MESSAGES",
            Self::ManageChannels => "MANAGE_CHANNELS",
            Self::ManageRoles => "MANAGE_ROLES",
            Self::InviteMembers => "INVITE_MEMBERS",
            Self::ManageSpace => "MANAGE_SPACE",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PermissionError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "VIEW_CHANNEL" => Ok(Self::ViewChannel),
            "SEND_MESSAGES" => Ok(Self::SendMessages),
            "MANAGE_MESSAGES" => Ok(Self::ManageMessages),
            "MANAGE_CHANNELS" => Ok(Self::ManageChannels),
            "MANAGE_ROLES" => Ok(Self::ManageRoles),
            "INVITE_MEMBERS" => Ok(Self::InviteMembers),
            "MANAGE_SPACE" => Ok(Self::ManageSpace),
            _ => Err(PermissionError::InvalidInput(
                "permission name is not supported",
            )),
        }
    }
}

pub fn permission_names(bitset: u64) -> Vec<&'static str> {
    [
        Permission::ViewChannel,
        Permission::SendMessages,
        Permission::ManageMessages,
        Permission::ManageChannels,
        Permission::ManageRoles,
        Permission::InviteMembers,
        Permission::ManageSpace,
    ]
    .into_iter()
    .filter(|permission| bitset & permission.bit() != 0)
    .map(Permission::as_str)
    .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Role {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub position: i32,
    pub permissions_bitset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleAssignment {
    pub space_id: Uuid,
    pub role_id: Uuid,
    pub user_id: Uuid,
    pub assigned_by_user_id: Uuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionTargetKind {
    Role,
    Member,
}

impl PermissionTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Member => "member",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PermissionError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "role" => Ok(Self::Role),
            "member" => Ok(Self::Member),
            _ => Err(PermissionError::InvalidInput(
                "permission override target_kind must be role or member",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelPermissionOverride {
    pub channel_id: Uuid,
    pub target_kind: PermissionTargetKind,
    pub target_id: Uuid,
    pub allow_bitset: u64,
    pub deny_bitset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignedRole {
    pub id: Uuid,
    pub permissions_bitset: u64,
}

#[derive(Debug)]
pub enum PermissionError {
    InvalidInput(&'static str),
    Forbidden,
    RoleAlreadyExists,
    RoleNotFound,
    StoreUnavailable,
}

impl PermissionError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::RoleAlreadyExists => StatusCode::CONFLICT,
            Self::RoleNotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::Forbidden => "forbidden",
            Self::RoleAlreadyExists => "role_already_exists",
            Self::RoleNotFound => "role_not_found",
            Self::StoreUnavailable => "permission_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::Forbidden => "permission denied",
            Self::RoleAlreadyExists => "role name already exists in this space",
            Self::RoleNotFound => "role was not found",
            Self::StoreUnavailable => "permission store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait PermissionStore: Send + Sync {
    async fn create_role(&self, role: Role) -> Result<(), PermissionError>;
    async fn get_role(
        &self,
        space_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<Role>, PermissionError>;
    async fn assign_role(&self, assignment: RoleAssignment) -> Result<(), PermissionError>;
    async fn assigned_roles_for_user(
        &self,
        space_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<AssignedRole>, PermissionError>;
    async fn upsert_channel_override(
        &self,
        permission_override: ChannelPermissionOverride,
    ) -> Result<(), PermissionError>;
    async fn channel_overrides_for_user(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        role_ids: &[Uuid],
    ) -> Result<Vec<ChannelPermissionOverride>, PermissionError>;
}

#[derive(Clone)]
pub struct PermissionService {
    store: std::sync::Arc<dyn PermissionStore>,
}

impl PermissionService {
    pub fn new(store: std::sync::Arc<dyn PermissionStore>) -> Self {
        Self { store }
    }

    pub async fn create_role(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        name: String,
        color: Option<String>,
        position: Option<i32>,
        permissions: Vec<String>,
    ) -> Result<Role, PermissionError> {
        let role = Role {
            id: ids::new_uuid_v7(),
            organization_id,
            space_id,
            name: normalize_role_name(name)?,
            color: normalize_color(color)?,
            position: position.unwrap_or(0).max(0),
            permissions_bitset: parse_permission_list(permissions)?,
        };

        self.store.create_role(role.clone()).await?;

        Ok(role)
    }

    pub async fn assign_role(
        &self,
        space_id: Uuid,
        role_id: Uuid,
        user_id: Uuid,
        assigned_by_user_id: Uuid,
    ) -> Result<RoleAssignment, PermissionError> {
        if self.store.get_role(space_id, role_id).await?.is_none() {
            return Err(PermissionError::RoleNotFound);
        }

        let assignment = RoleAssignment {
            space_id,
            role_id,
            user_id,
            assigned_by_user_id,
        };

        self.store.assign_role(assignment.clone()).await?;

        Ok(assignment)
    }

    pub async fn set_channel_override(
        &self,
        channel_id: Uuid,
        target_kind: PermissionTargetKind,
        target_id: Uuid,
        allow: Vec<String>,
        deny: Vec<String>,
    ) -> Result<ChannelPermissionOverride, PermissionError> {
        let permission_override = ChannelPermissionOverride {
            channel_id,
            target_kind,
            target_id,
            allow_bitset: parse_permission_list(allow)?,
            deny_bitset: parse_permission_list(deny)?,
        };

        self.store
            .upsert_channel_override(permission_override.clone())
            .await?;

        Ok(permission_override)
    }

    pub async fn require_space(
        &self,
        user_id: Uuid,
        space: &SpaceMembership,
        permission: Permission,
    ) -> Result<(), PermissionError> {
        if self.can_in_space(user_id, space, permission).await? {
            Ok(())
        } else {
            Err(PermissionError::Forbidden)
        }
    }

    pub async fn require_channel(
        &self,
        user_id: Uuid,
        space: &SpaceMembership,
        channel: &Channel,
        permission: Permission,
    ) -> Result<(), PermissionError> {
        if self
            .can_in_channel(user_id, space, channel, permission)
            .await?
        {
            Ok(())
        } else {
            Err(PermissionError::Forbidden)
        }
    }

    pub async fn can_in_channel(
        &self,
        user_id: Uuid,
        space: &SpaceMembership,
        channel: &Channel,
        permission: Permission,
    ) -> Result<bool, PermissionError> {
        if is_admin_role(&space.role) {
            return Ok(true);
        }

        let assigned_roles = self
            .store
            .assigned_roles_for_user(space.id, user_id)
            .await?;
        let mut effective = builtin_permissions_for_role(&space.role);
        let role_ids = assigned_roles
            .iter()
            .map(|role| {
                effective |= role.permissions_bitset;
                role.id
            })
            .collect::<Vec<_>>();

        let overrides = self
            .store
            .channel_overrides_for_user(channel.id, user_id, &role_ids)
            .await?;
        effective = apply_channel_overrides(effective, user_id, &role_ids, overrides);

        Ok(effective & permission.bit() != 0)
    }

    async fn can_in_space(
        &self,
        user_id: Uuid,
        space: &SpaceMembership,
        permission: Permission,
    ) -> Result<bool, PermissionError> {
        if is_admin_role(&space.role) {
            return Ok(true);
        }

        let assigned_roles = self
            .store
            .assigned_roles_for_user(space.id, user_id)
            .await?;
        let effective = assigned_roles
            .into_iter()
            .fold(builtin_permissions_for_role(&space.role), |bitset, role| {
                bitset | role.permissions_bitset
            });

        Ok(effective & permission.bit() != 0)
    }
}

fn builtin_permissions_for_role(role: &str) -> u64 {
    match role {
        "owner" | "admin" => ALL_MINIMAL_PERMISSIONS,
        "member" => VIEW_CHANNEL | SEND_MESSAGES,
        "guest" => VIEW_CHANNEL,
        _ => 0,
    }
}

fn is_admin_role(role: &str) -> bool {
    matches!(role, "owner" | "admin")
}

fn apply_channel_overrides(
    mut effective: u64,
    user_id: Uuid,
    role_ids: &[Uuid],
    overrides: Vec<ChannelPermissionOverride>,
) -> u64 {
    let mut role_allow = 0;
    let mut role_deny = 0;
    let mut member_allow = 0;
    let mut member_deny = 0;

    for permission_override in overrides {
        match permission_override.target_kind {
            PermissionTargetKind::Role if role_ids.contains(&permission_override.target_id) => {
                role_allow |= permission_override.allow_bitset;
                role_deny |= permission_override.deny_bitset;
            }
            PermissionTargetKind::Member if permission_override.target_id == user_id => {
                member_allow |= permission_override.allow_bitset;
                member_deny |= permission_override.deny_bitset;
            }
            _ => {}
        }
    }

    effective &= !role_deny;
    effective |= role_allow & !role_deny;
    effective &= !member_deny;
    effective |= member_allow & !member_deny;

    effective
}

fn normalize_role_name(name: String) -> Result<String, PermissionError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=80).contains(&name.len()) {
        Ok(name)
    } else {
        Err(PermissionError::InvalidInput(
            "role name must be between 1 and 80 characters",
        ))
    }
}

fn normalize_color(color: Option<String>) -> Result<Option<String>, PermissionError> {
    let Some(color) = color else {
        return Ok(None);
    };
    let color = color.trim().to_owned();
    let valid_hex_color = color.len() == 7
        && color.starts_with('#')
        && color.chars().skip(1).all(|character| {
            character.is_ascii_digit() || matches!(character, 'a'..='f' | 'A'..='F')
        });

    if valid_hex_color {
        Ok(Some(color))
    } else {
        Err(PermissionError::InvalidInput(
            "role color must be a hex color like #5865f2",
        ))
    }
}

fn parse_permission_list(permissions: Vec<String>) -> Result<u64, PermissionError> {
    permissions.into_iter().try_fold(0, |bitset, permission| {
        Permission::parse(&permission).map(|permission| bitset | permission.bit())
    })
}
