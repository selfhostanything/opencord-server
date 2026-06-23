use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::audit::{AuditError, NewAuditEvent};
use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::organization::OrganizationError;
use crate::domain::permission::{
    ChannelPermissionOverride, Permission, PermissionError, PermissionTargetKind, Role,
    RoleAssignment, permission_names,
};
use crate::domain::space::{SpaceError, SpaceMembership};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::permission::{
    AddSpaceMemberRequest, AssignRoleRequest, ChannelPermissionOverrideResourceResponse,
    ChannelPermissionOverrideResponse, CreateRoleRequest, RoleAssignmentResourceResponse,
    RoleAssignmentResponse, RoleResourceResponse, RoleResponse,
    SetChannelPermissionOverrideRequest, SpaceMemberDetailResponse, SpaceMemberResourceResponse,
};
use crate::state::AppState;

pub async fn add_space_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    Json(request): Json<AddSpaceMemberRequest>,
) -> Result<impl IntoResponse, PermissionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &space, Permission::InviteMembers)
        .await?;

    let user_id = parse_uuid(&request.user_id, "valid user_id is required")?;
    state
        .organizations
        .add_member_if_missing(space.organization_id, user_id, "member".to_owned())
        .await?;
    let member = state
        .spaces
        .add_member(
            space.id,
            user_id,
            request.role.unwrap_or_else(|| "member".to_owned()),
        )
        .await?;
    let member_role = member.role.clone();
    state
        .audit
        .record(NewAuditEvent {
            organization_id: space.organization_id,
            space_id: space.id,
            actor_user_id: user.id,
            action: "space.member.added",
            target_type: "user",
            target_id: user_id,
            metadata: json!({ "role": member_role }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(SpaceMemberResourceResponse {
            member: SpaceMemberDetailResponse {
                space_id: member.id.to_string(),
                user_id: user_id.to_string(),
                role: member.role,
                status: "active".to_owned(),
            },
        }),
    ))
}

pub async fn create_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    Json(request): Json<CreateRoleRequest>,
) -> Result<impl IntoResponse, PermissionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &space, Permission::ManageRoles)
        .await?;

    let role = state
        .permissions
        .create_role(
            space.organization_id,
            space.id,
            request.name,
            request.color,
            request.position,
            request.permissions,
        )
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: space.organization_id,
            space_id: space.id,
            actor_user_id: user.id,
            action: "role.created",
            target_type: "role",
            target_id: role.id,
            metadata: json!({
                "name": role.name.clone(),
                "permissions": permission_names(role.permissions_bitset)
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(RoleResourceResponse {
            role: RoleResponse::from(role),
        }),
    ))
}

pub async fn assign_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((space_id, role_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AssignRoleRequest>,
) -> Result<impl IntoResponse, PermissionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let space = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &space, Permission::ManageRoles)
        .await?;

    let user_id = parse_uuid(&request.user_id, "valid user_id is required")?;
    state.spaces.get_for_user(user_id, space.id).await?;
    let assignment = state
        .permissions
        .assign_role(space.id, role_id, user_id, user.id)
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: space.organization_id,
            space_id: space.id,
            actor_user_id: user.id,
            action: "role.assigned",
            target_type: "role_assignment",
            target_id: user_id,
            metadata: json!({
                "role_id": role_id,
                "user_id": user_id,
                "assigned_by_user_id": user.id
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(RoleAssignmentResourceResponse {
            assignment: RoleAssignmentResponse::from(assignment),
        }),
    ))
}

pub async fn set_channel_override(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(request): Json<SetChannelPermissionOverrideRequest>,
) -> Result<Json<ChannelPermissionOverrideResourceResponse>, PermissionApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ManageChannels)
        .await?;

    let target_kind = PermissionTargetKind::parse(&request.target_kind)?;
    let target_id = parse_uuid(&request.target_id, "valid target_id is required")?;
    let permission_override = state
        .permissions
        .set_channel_override(
            channel.id,
            target_kind,
            target_id,
            request.allow,
            request.deny,
        )
        .await?;
    state
        .audit
        .record(NewAuditEvent {
            organization_id: channel.organization_id,
            space_id: channel.space_id,
            actor_user_id: user.id,
            action: "channel.permission_override.set",
            target_type: "channel",
            target_id: channel.id,
            metadata: json!({
                "target_kind": permission_override.target_kind.as_str(),
                "target_id": permission_override.target_id,
                "allow": permission_names(permission_override.allow_bitset),
                "deny": permission_names(permission_override.deny_bitset)
            }),
        })
        .await?;

    Ok(Json(ChannelPermissionOverrideResourceResponse {
        permission_override: ChannelPermissionOverrideResponse::from(permission_override),
    }))
}

#[derive(Debug)]
pub enum PermissionApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Space(SpaceError),
    Channel(ChannelError),
    Permission(PermissionError),
    Audit(AuditError),
}

impl From<AuthError> for PermissionApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for PermissionApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<SpaceError> for PermissionApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<ChannelError> for PermissionApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<PermissionError> for PermissionApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<AuditError> for PermissionApiError {
    fn from(error: AuditError) -> Self {
        Self::Audit(error)
    }
}

impl IntoResponse for PermissionApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Audit(error) => (error.status_code(), error.code(), error.message()),
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorDetail { code, message },
            }),
        )
            .into_response()
    }
}

fn parse_uuid(value: &str, message: &'static str) -> Result<Uuid, PermissionError> {
    Uuid::parse_str(value).map_err(|_| PermissionError::InvalidInput(message))
}

impl From<SpaceMembership> for SpaceMemberDetailResponse {
    fn from(membership: SpaceMembership) -> Self {
        Self {
            space_id: membership.id.to_string(),
            user_id: String::new(),
            role: membership.role,
            status: "active".to_owned(),
        }
    }
}

impl From<Role> for RoleResourceResponse {
    fn from(role: Role) -> Self {
        Self {
            role: RoleResponse::from(role),
        }
    }
}

impl From<RoleAssignment> for RoleAssignmentResourceResponse {
    fn from(assignment: RoleAssignment) -> Self {
        Self {
            assignment: RoleAssignmentResponse::from(assignment),
        }
    }
}

impl From<ChannelPermissionOverride> for ChannelPermissionOverrideResourceResponse {
    fn from(permission_override: ChannelPermissionOverride) -> Self {
        Self {
            permission_override: ChannelPermissionOverrideResponse::from(permission_override),
        }
    }
}
