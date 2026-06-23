use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::meeting::{
    MeetingAttendee, MeetingBundle, MeetingPatch, MeetingReminder, NewMeetingAttendee,
    NewMeetingReminder,
};

#[derive(Debug, Deserialize)]
pub struct CreateMeetingRequest {
    pub space_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: Option<String>,
    #[serde(default)]
    pub attendees: Vec<CreateMeetingAttendeeRequest>,
    #[serde(default)]
    pub reminders: Vec<CreateMeetingReminderRequest>,
}

#[derive(Debug, Deserialize)]
pub struct PatchMeetingRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMeetingAttendeeRequest {
    pub user_id: Option<Uuid>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMeetingReminderRequest {
    pub recipient_user_id: Option<Uuid>,
    pub recipient_email: Option<String>,
    pub channel: String,
    pub offset_minutes: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeetingResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: Option<String>,
    pub channel_id: Option<String>,
    pub created_by_user_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: String,
    pub join_slug: String,
    pub join_url: String,
    pub cancelled_at: Option<String>,
    pub attendees: Vec<MeetingAttendeeResponse>,
    pub reminders: Vec<MeetingReminderResponse>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeetingAttendeeResponse {
    pub id: String,
    pub meeting_id: String,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    pub response_status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeetingReminderResponse {
    pub id: String,
    pub meeting_id: String,
    pub recipient_user_id: Option<String>,
    pub recipient_email: Option<String>,
    pub channel: String,
    pub offset_minutes: i32,
    pub scheduled_for: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct MeetingResourceResponse {
    pub meeting: MeetingResponse,
}

#[derive(Debug, Serialize)]
pub struct MeetingListResponse {
    pub meetings: Vec<MeetingResponse>,
}

impl From<PatchMeetingRequest> for MeetingPatch {
    fn from(request: PatchMeetingRequest) -> Self {
        Self {
            title: request.title,
            description: request.description,
            starts_at: request.starts_at,
            ends_at: request.ends_at,
            timezone: request.timezone,
        }
    }
}

impl From<CreateMeetingAttendeeRequest> for NewMeetingAttendee {
    fn from(request: CreateMeetingAttendeeRequest) -> Self {
        Self {
            user_id: request.user_id,
            email: request.email,
            display_name: request.display_name,
            role: request.role,
        }
    }
}

impl From<CreateMeetingReminderRequest> for NewMeetingReminder {
    fn from(request: CreateMeetingReminderRequest) -> Self {
        Self {
            recipient_user_id: request.recipient_user_id,
            recipient_email: request.recipient_email,
            channel: request.channel,
            offset_minutes: request.offset_minutes,
        }
    }
}

impl From<MeetingBundle> for MeetingResponse {
    fn from(bundle: MeetingBundle) -> Self {
        Self::from_bundle(bundle, "")
    }
}

impl MeetingResponse {
    pub fn from_bundle(bundle: MeetingBundle, public_url: &str) -> Self {
        let join_url = join_url(public_url, &bundle.meeting.join_slug);
        Self {
            id: bundle.meeting.id.to_string(),
            organization_id: bundle.meeting.organization_id.to_string(),
            space_id: bundle.meeting.space_id.map(|id| id.to_string()),
            channel_id: bundle.meeting.channel_id.map(|id| id.to_string()),
            created_by_user_id: bundle.meeting.created_by_user_id.to_string(),
            title: bundle.meeting.title,
            description: bundle.meeting.description,
            status: bundle.meeting.status,
            starts_at: bundle.meeting.starts_at,
            ends_at: bundle.meeting.ends_at,
            timezone: bundle.meeting.timezone,
            join_slug: bundle.meeting.join_slug,
            join_url,
            cancelled_at: bundle.meeting.cancelled_at,
            attendees: bundle
                .attendees
                .into_iter()
                .map(MeetingAttendeeResponse::from)
                .collect(),
            reminders: bundle
                .reminders
                .into_iter()
                .map(MeetingReminderResponse::from)
                .collect(),
        }
    }
}

fn join_url(public_url: &str, join_slug: &str) -> String {
    let public_url = public_url.trim_end_matches('/');
    if public_url.is_empty() {
        format!("/join/{join_slug}")
    } else {
        format!("{public_url}/join/{join_slug}")
    }
}

impl From<MeetingAttendee> for MeetingAttendeeResponse {
    fn from(attendee: MeetingAttendee) -> Self {
        Self {
            id: attendee.id.to_string(),
            meeting_id: attendee.meeting_id.to_string(),
            user_id: attendee.user_id.map(|id| id.to_string()),
            email: attendee.email,
            display_name: attendee.display_name,
            role: attendee.role,
            response_status: attendee.response_status,
        }
    }
}

impl From<MeetingReminder> for MeetingReminderResponse {
    fn from(reminder: MeetingReminder) -> Self {
        Self {
            id: reminder.id.to_string(),
            meeting_id: reminder.meeting_id.to_string(),
            recipient_user_id: reminder.recipient_user_id.map(|id| id.to_string()),
            recipient_email: reminder.recipient_email,
            channel: reminder.channel,
            offset_minutes: reminder.offset_minutes,
            scheduled_for: reminder.scheduled_for,
            status: reminder.status,
        }
    }
}
