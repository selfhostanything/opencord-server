use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthUser};
use crate::domain::calendar::meeting_invite_ics;
use crate::domain::calendar_sync::CalendarSyncError;
use crate::domain::channel::ChannelError;
use crate::domain::media::{IssueMediaRoomToken, MediaError, MediaRoomType};
use crate::domain::meeting::{MeetingBundle, MeetingError};
use crate::domain::organization::{OrganizationError, OrganizationMembership};
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::space::SpaceError;
use crate::http::session::bearer_token;
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::models::calendar::{CalendarEventResourceResponse, CalendarEventResponse};
use crate::models::media::{MediaRoomTokenResourceResponse, MediaRoomTokenResponse};
use crate::models::meeting::{
    CreateMeetingMediaTokenRequest, CreateMeetingRequest, MeetingListResponse,
    MeetingResourceResponse, MeetingResponse, PatchMeetingRequest,
};
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(request): Json<CreateMeetingRequest>,
) -> Result<impl IntoResponse, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;
    validate_meeting_context(
        &state,
        &user,
        organization_id,
        request.space_id,
        request.channel_id,
    )
    .await?;

    let meeting = state
        .meetings
        .create(
            organization_id,
            request.space_id,
            request.channel_id,
            user.id,
            request.title,
            request.description,
            request.starts_at,
            request.ends_at,
            request.timezone,
            request.attendees.into_iter().map(Into::into).collect(),
            request.reminders.into_iter().map(Into::into).collect(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(MeetingResourceResponse {
            meeting: meeting_response(meeting, &state),
        }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<MeetingListResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    state
        .organizations
        .get_for_user(user.id, organization_id)
        .await?;

    let meetings = state
        .meetings
        .list_for_organization(organization_id)
        .await?;
    let mut visible = Vec::new();
    for meeting in meetings {
        if can_read_meeting(&state, &user, &meeting).await? {
            visible.push(meeting_response(meeting, &state));
        }
    }

    Ok(Json(MeetingListResponse { meetings: visible }))
}

pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    require_read_meeting(&state, &user, &meeting).await?;

    Ok(Json(MeetingResourceResponse {
        meeting: meeting_response(meeting, &state),
    }))
}

pub async fn invite_ics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<impl IntoResponse, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    require_read_meeting(&state, &user, &meeting).await?;

    let body = meeting_invite_ics(&meeting, &state.config.public_url, Utc::now())?;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );

    Ok((response_headers, body))
}

pub async fn create_media_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
    Json(request): Json<CreateMeetingMediaTokenRequest>,
) -> Result<(StatusCode, Json<MediaRoomTokenResourceResponse>), MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    require_read_meeting(&state, &user, &meeting).await?;
    if meeting.meeting.status != "scheduled" {
        return Err(MediaError::InvalidInput(
            "meeting media is unavailable for cancelled meetings",
        )
        .into());
    }

    let grants = request.grants();
    grants.validate()?;
    enforce_meeting_media_permissions(&state, &user, &meeting, grants).await?;

    let media = state.media.issue_room_token(IssueMediaRoomToken {
        room_type: MediaRoomType::MeetingRoom,
        organization_id: meeting.meeting.organization_id,
        space_id: meeting.meeting.space_id,
        channel_id: meeting.meeting.channel_id,
        meeting_id: Some(meeting.meeting.id),
        participant_user_id: user.id,
        grants,
    })?;

    Ok((
        StatusCode::CREATED,
        Json(MediaRoomTokenResourceResponse {
            media: MediaRoomTokenResponse::from(media),
        }),
    ))
}

pub async fn resolve_join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(join_slug): Path<String>,
) -> Result<Json<MeetingResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get_by_join_slug(join_slug).await?;
    require_read_meeting(&state, &user, &meeting).await?;

    Ok(Json(MeetingResourceResponse {
        meeting: meeting_response(meeting, &state),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
    Json(request): Json<PatchMeetingRequest>,
) -> Result<Json<MeetingResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    let organization = require_read_meeting(&state, &user, &meeting).await?;
    require_meeting_manager(&user, &organization, &meeting)?;

    let meeting = state.meetings.update(meeting, request.into()).await?;

    Ok(Json(MeetingResourceResponse {
        meeting: meeting_response(meeting, &state),
    }))
}

pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    let organization = require_read_meeting(&state, &user, &meeting).await?;
    require_meeting_manager(&user, &organization, &meeting)?;

    let meeting = state.meetings.cancel(meeting).await?;

    Ok(Json(MeetingResourceResponse {
        meeting: meeting_response(meeting, &state),
    }))
}

pub async fn sync_google_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<CalendarEventResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    let organization = require_read_meeting(&state, &user, &meeting).await?;
    require_meeting_manager(&user, &organization, &meeting)?;

    let calendar_event = state
        .calendar_sync
        .sync_google_meeting(user.id, meeting, &state.config.public_url)
        .await?;

    Ok(Json(CalendarEventResourceResponse {
        calendar_event: CalendarEventResponse::from(calendar_event),
    }))
}

pub async fn sync_microsoft_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<CalendarEventResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    let organization = require_read_meeting(&state, &user, &meeting).await?;
    require_meeting_manager(&user, &organization, &meeting)?;

    let calendar_event = state
        .calendar_sync
        .sync_microsoft_meeting(user.id, meeting, &state.config.public_url)
        .await?;

    Ok(Json(CalendarEventResourceResponse {
        calendar_event: CalendarEventResponse::from(calendar_event),
    }))
}

pub async fn sync_caldav_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<CalendarEventResourceResponse>, MeetingApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let meeting = state.meetings.get(meeting_id).await?;
    let organization = require_read_meeting(&state, &user, &meeting).await?;
    require_meeting_manager(&user, &organization, &meeting)?;

    let calendar_event = state
        .calendar_sync
        .sync_caldav_meeting(user.id, meeting, &state.config.public_url)
        .await?;

    Ok(Json(CalendarEventResourceResponse {
        calendar_event: CalendarEventResponse::from(calendar_event),
    }))
}

fn meeting_response(meeting: MeetingBundle, state: &AppState) -> MeetingResponse {
    MeetingResponse::from_bundle(meeting, &state.config.public_url)
}

async fn validate_meeting_context(
    state: &AppState,
    user: &AuthUser,
    organization_id: Uuid,
    space_id: Option<Uuid>,
    channel_id: Option<Uuid>,
) -> Result<(), MeetingApiError> {
    let Some(space_id) = space_id else {
        if channel_id.is_some() {
            return Err(MeetingError::InvalidInput("meeting channel requires space_id").into());
        }
        return Ok(());
    };

    let space = state.spaces.get_for_user(user.id, space_id).await?;
    if space.organization_id != organization_id {
        return Err(MeetingError::InvalidInput(
            "meeting space must belong to the requested organization",
        )
        .into());
    }

    if let Some(channel_id) = channel_id {
        let channel = state.channels.get(channel_id).await?;
        if channel.organization_id != organization_id || channel.space_id != space_id {
            return Err(MeetingError::InvalidInput(
                "meeting channel must belong to the requested organization and space",
            )
            .into());
        }
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::ViewChannel)
            .await?;
    }

    Ok(())
}

async fn require_read_meeting(
    state: &AppState,
    user: &AuthUser,
    meeting: &MeetingBundle,
) -> Result<OrganizationMembership, MeetingApiError> {
    let organization = state
        .organizations
        .get_for_user(user.id, meeting.meeting.organization_id)
        .await?;
    if can_read_meeting(state, user, meeting).await? {
        Ok(organization)
    } else {
        Err(MeetingError::NotFound.into())
    }
}

async fn can_read_meeting(
    state: &AppState,
    user: &AuthUser,
    meeting: &MeetingBundle,
) -> Result<bool, MeetingApiError> {
    let Some(space_id) = meeting.meeting.space_id else {
        return Ok(true);
    };
    let space = match state.spaces.get_for_user(user.id, space_id).await {
        Ok(space) => space,
        Err(SpaceError::NotFound) => return Ok(false),
        Err(error) => return Err(error.into()),
    };

    let Some(channel_id) = meeting.meeting.channel_id else {
        return Ok(true);
    };
    let channel = match state.channels.get(channel_id).await {
        Ok(channel) => channel,
        Err(ChannelError::NotFound) => return Ok(false),
        Err(error) => return Err(error.into()),
    };

    Ok(state
        .permissions
        .can_in_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?)
}

async fn enforce_meeting_media_permissions(
    state: &AppState,
    user: &AuthUser,
    meeting: &MeetingBundle,
    grants: crate::domain::media::MediaTokenGrants,
) -> Result<(), MeetingApiError> {
    let Some(space_id) = meeting.meeting.space_id else {
        if meeting.meeting.channel_id.is_some() {
            return Err(
                MediaError::InvalidInput("meeting channel requires space_id for media").into(),
            );
        }
        return Ok(());
    };

    let space = state.spaces.get_for_user(user.id, space_id).await?;
    if space.organization_id != meeting.meeting.organization_id {
        return Err(MediaError::InvalidInput(
            "meeting media space must belong to the meeting organization",
        )
        .into());
    }

    let Some(channel_id) = meeting.meeting.channel_id else {
        return Ok(());
    };
    let channel = state.channels.get(channel_id).await?;
    if channel.organization_id != meeting.meeting.organization_id || channel.space_id != space_id {
        return Err(MediaError::InvalidInput(
            "meeting media channel must belong to the meeting organization and space",
        )
        .into());
    }

    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ConnectVoice)
        .await?;
    if grants.can_publish_audio {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::Speak)
            .await?;
    }
    if grants.can_publish_video {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::UseVideo)
            .await?;
    }
    if grants.can_publish_screen {
        state
            .permissions
            .require_channel(user.id, &space, &channel, Permission::ShareScreen)
            .await?;
    }

    Ok(())
}

fn require_meeting_manager(
    user: &AuthUser,
    organization: &OrganizationMembership,
    meeting: &MeetingBundle,
) -> Result<(), MeetingApiError> {
    if meeting.meeting.created_by_user_id == user.id
        || organization.role == "owner"
        || organization.role == "admin"
    {
        Ok(())
    } else {
        Err(PermissionError::Forbidden.into())
    }
}

#[derive(Debug)]
pub enum MeetingApiError {
    Auth(AuthError),
    Calendar(CalendarSyncError),
    Media(MediaError),
    Organization(OrganizationError),
    Space(SpaceError),
    Channel(ChannelError),
    Permission(PermissionError),
    Meeting(MeetingError),
}

impl From<AuthError> for MeetingApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<CalendarSyncError> for MeetingApiError {
    fn from(error: CalendarSyncError) -> Self {
        Self::Calendar(error)
    }
}

impl From<MediaError> for MeetingApiError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<OrganizationError> for MeetingApiError {
    fn from(error: OrganizationError) -> Self {
        Self::Organization(error)
    }
}

impl From<SpaceError> for MeetingApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<ChannelError> for MeetingApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<PermissionError> for MeetingApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<MeetingError> for MeetingApiError {
    fn from(error: MeetingError) -> Self {
        Self::Meeting(error)
    }
}

impl IntoResponse for MeetingApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Calendar(error) => (error.status_code(), error.code(), error.message()),
            Self::Media(error) => (error.status_code(), error.code(), error.message()),
            Self::Organization(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Meeting(error) => (error.status_code(), error.code(), error.message()),
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
