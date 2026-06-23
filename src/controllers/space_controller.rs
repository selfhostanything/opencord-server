use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::domain::auth::AuthError;
use crate::domain::organization::OrganizationError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::realtime::RealtimeEvent;
use crate::domain::space::{SpaceError, SpaceMembership};
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::space::{
    CreateSpaceRequest, PatchSpaceRequest, SpaceListResponse, SpaceMembershipResponse,
    SpaceResponse,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<CreateSpaceRequest>,
) -> Result<impl IntoResponse, SpaceApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;

    let space = state
        .spaces
        .create(user, organization_id, request.name)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(SpaceMembershipResponse::from(space)),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<SpaceListResponse>, SpaceApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;

    let spaces = state.spaces.list_for_user(user.id, organization_id).await?;

    Ok(Json(SpaceListResponse {
        spaces: spaces.into_iter().map(SpaceResponse::from).collect(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    Json(request): Json<PatchSpaceRequest>,
) -> Result<Json<SpaceMembershipResponse>, SpaceApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let existing = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &existing, Permission::ManageSpace)
        .await?;

    let space = state.spaces.update(existing, request.into()).await?;
    let response = SpaceMembershipResponse::from(space.clone());
    state.realtime.publish(RealtimeEvent::space(
        "space.updated",
        space.organization_id,
        space.id,
        json!({
            "guild": {
                "id": space.id.to_string(),
                "name": space.name,
                "unavailable": false
            }
        }),
    ));

    Ok(Json(response))
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<StatusCode, SpaceApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let existing = state.spaces.get_for_user(user.id, space_id).await?;
    state
        .permissions
        .require_space(user.id, &existing, Permission::ManageSpace)
        .await?;

    let member_user_ids = state.spaces.active_member_user_ids(existing.id).await?;
    let space = state.spaces.delete(existing).await?;
    let member_user_ids = member_user_ids
        .into_iter()
        .map(|user_id| user_id.to_string())
        .collect::<Vec<_>>();

    state.realtime.publish(RealtimeEvent::space(
        "space.deleted",
        space.organization_id,
        space.id,
        json!({
            "guild": {
                "id": space.id.to_string(),
                "name": space.name,
                "unavailable": false
            },
            "member_user_ids": member_user_ids
        }),
    ));

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug)]
pub enum SpaceApiError {
    Auth(AuthError),
    Organization(OrganizationError),
    Space(SpaceError),
    Permission(PermissionError),
}

impl From<AuthError> for SpaceApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<OrganizationError> for SpaceApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<SpaceError> for SpaceApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for SpaceApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl IntoResponse for SpaceApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
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

impl From<SpaceMembership> for SpaceMembershipResponse {
    fn from(membership: SpaceMembership) -> Self {
        Self {
            space: SpaceResponse::from(membership.clone()),
            membership: crate::models::space::SpaceMemberResponse {
                role: membership.role,
            },
        }
    }
}
