use axum::http::StatusCode;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Meeting {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub created_by_user_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: String,
    pub join_slug: String,
    pub cancelled_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingAttendee {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub user_id: Option<Uuid>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    pub response_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingReminder {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub recipient_user_id: Option<Uuid>,
    pub recipient_email: Option<String>,
    pub channel: String,
    pub offset_minutes: i32,
    pub scheduled_for: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingBundle {
    pub meeting: Meeting,
    pub attendees: Vec<MeetingAttendee>,
    pub reminders: Vec<MeetingReminder>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingReminderJob {
    pub meeting: Meeting,
    pub reminder: MeetingReminder,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MeetingPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMeetingAttendee {
    pub user_id: Option<Uuid>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMeetingReminder {
    pub recipient_user_id: Option<Uuid>,
    pub recipient_email: Option<String>,
    pub channel: String,
    pub offset_minutes: i32,
}

#[derive(Debug)]
pub enum MeetingError {
    InvalidInput(&'static str),
    NotFound,
    StoreUnavailable,
}

impl MeetingError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::NotFound => "meeting_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::NotFound => "meeting was not found",
            Self::StoreUnavailable => "meeting store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait MeetingStore: Send + Sync {
    async fn create_meeting(&self, bundle: MeetingBundle) -> Result<(), MeetingError>;
    async fn list_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<MeetingBundle>, MeetingError>;
    async fn get_meeting(&self, meeting_id: Uuid) -> Result<Option<MeetingBundle>, MeetingError>;
    async fn get_meeting_by_join_slug(
        &self,
        join_slug: String,
    ) -> Result<Option<MeetingBundle>, MeetingError>;
    async fn update_meeting(&self, meeting: Meeting) -> Result<MeetingBundle, MeetingError>;
    async fn list_due_reminders(
        &self,
        due_at: String,
        limit: usize,
    ) -> Result<Vec<MeetingReminderJob>, MeetingError>;
    async fn mark_reminder_sent(
        &self,
        reminder_id: Uuid,
        sent_at: String,
    ) -> Result<(), MeetingError>;
    async fn mark_reminder_failed(
        &self,
        reminder_id: Uuid,
        failed_at: String,
        failure_reason: String,
    ) -> Result<(), MeetingError>;
}

#[derive(Clone)]
pub struct MeetingService {
    store: std::sync::Arc<dyn MeetingStore>,
}

impl MeetingService {
    pub fn new(store: std::sync::Arc<dyn MeetingStore>) -> Self {
        Self { store }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        organization_id: Uuid,
        space_id: Option<Uuid>,
        channel_id: Option<Uuid>,
        created_by_user_id: Uuid,
        title: String,
        description: Option<String>,
        starts_at: String,
        ends_at: String,
        timezone: Option<String>,
        attendees: Vec<NewMeetingAttendee>,
        reminders: Vec<NewMeetingReminder>,
    ) -> Result<MeetingBundle, MeetingError> {
        let meeting_id = ids::new_uuid_v7();
        let starts_at = parse_time(starts_at, "meeting starts_at must be an RFC3339 timestamp")?;
        let ends_at = parse_time(ends_at, "meeting ends_at must be an RFC3339 timestamp")?;
        validate_schedule(starts_at, ends_at)?;

        let meeting = Meeting {
            id: meeting_id,
            organization_id,
            space_id,
            channel_id,
            created_by_user_id,
            title: normalize_title(title)?,
            description: normalize_description(description)?,
            status: "scheduled".to_owned(),
            starts_at: format_time(starts_at),
            ends_at: format_time(ends_at),
            timezone: normalize_timezone(timezone)?,
            join_slug: format!("mtg-{}", meeting_id.simple()),
            cancelled_at: None,
        };
        let attendees = attendees
            .into_iter()
            .map(|attendee| normalize_attendee(meeting_id, attendee))
            .collect::<Result<Vec<_>, _>>()?;
        let reminders = reminders
            .into_iter()
            .map(|reminder| normalize_reminder(meeting_id, starts_at, reminder))
            .collect::<Result<Vec<_>, _>>()?;
        let bundle = MeetingBundle {
            meeting,
            attendees,
            reminders,
        };

        self.store.create_meeting(bundle.clone()).await?;

        Ok(bundle)
    }

    pub async fn list_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<MeetingBundle>, MeetingError> {
        self.store.list_for_organization(organization_id).await
    }

    pub async fn get(&self, meeting_id: Uuid) -> Result<MeetingBundle, MeetingError> {
        self.store
            .get_meeting(meeting_id)
            .await?
            .ok_or(MeetingError::NotFound)
    }

    pub async fn get_by_join_slug(&self, join_slug: String) -> Result<MeetingBundle, MeetingError> {
        self.store
            .get_meeting_by_join_slug(join_slug)
            .await?
            .ok_or(MeetingError::NotFound)
    }

    pub async fn update(
        &self,
        mut bundle: MeetingBundle,
        patch: MeetingPatch,
    ) -> Result<MeetingBundle, MeetingError> {
        if let Some(title) = patch.title {
            bundle.meeting.title = normalize_title(title)?;
        }
        if let Some(description) = patch.description {
            bundle.meeting.description = normalize_description(Some(description))?;
        }
        if let Some(timezone) = patch.timezone {
            bundle.meeting.timezone = normalize_timezone(Some(timezone))?;
        }

        let starts_at = match patch.starts_at {
            Some(starts_at) => {
                parse_time(starts_at, "meeting starts_at must be an RFC3339 timestamp")?
            }
            None => parse_time(
                bundle.meeting.starts_at.clone(),
                "meeting starts_at must be an RFC3339 timestamp",
            )?,
        };
        let ends_at = match patch.ends_at {
            Some(ends_at) => parse_time(ends_at, "meeting ends_at must be an RFC3339 timestamp")?,
            None => parse_time(
                bundle.meeting.ends_at.clone(),
                "meeting ends_at must be an RFC3339 timestamp",
            )?,
        };
        validate_schedule(starts_at, ends_at)?;
        bundle.meeting.starts_at = format_time(starts_at);
        bundle.meeting.ends_at = format_time(ends_at);

        self.store.update_meeting(bundle.meeting).await
    }

    pub async fn cancel(&self, mut bundle: MeetingBundle) -> Result<MeetingBundle, MeetingError> {
        bundle.meeting.status = "cancelled".to_owned();
        bundle.meeting.cancelled_at = Some(format_time(Utc::now()));

        self.store.update_meeting(bundle.meeting).await
    }
}

fn normalize_title(title: String) -> Result<String, MeetingError> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=200).contains(&title.len()) {
        Ok(title)
    } else {
        Err(MeetingError::InvalidInput(
            "meeting title must be between 1 and 200 characters",
        ))
    }
}

fn normalize_description(description: Option<String>) -> Result<Option<String>, MeetingError> {
    let Some(description) = description else {
        return Ok(None);
    };
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if description.len() > 4000 {
        Err(MeetingError::InvalidInput(
            "meeting description must be 4000 characters or fewer",
        ))
    } else if description.is_empty() {
        Ok(None)
    } else {
        Ok(Some(description))
    }
}

fn normalize_timezone(timezone: Option<String>) -> Result<String, MeetingError> {
    let timezone = timezone
        .unwrap_or_else(|| "UTC".to_owned())
        .trim()
        .to_owned();
    if (1..=64).contains(&timezone.len()) {
        Ok(timezone)
    } else {
        Err(MeetingError::InvalidInput(
            "meeting timezone must be between 1 and 64 characters",
        ))
    }
}

fn normalize_attendee(
    meeting_id: Uuid,
    attendee: NewMeetingAttendee,
) -> Result<MeetingAttendee, MeetingError> {
    let email = attendee.email.map(normalize_email).transpose()?;
    if attendee.user_id.is_none() && email.is_none() {
        return Err(MeetingError::InvalidInput(
            "meeting attendee requires user_id or email",
        ));
    }

    Ok(MeetingAttendee {
        id: ids::new_uuid_v7(),
        meeting_id,
        user_id: attendee.user_id,
        email,
        display_name: normalize_description(attendee.display_name)?,
        role: normalize_attendee_role(attendee.role)?,
        response_status: "needs_action".to_owned(),
    })
}

fn normalize_reminder(
    meeting_id: Uuid,
    starts_at: DateTime<Utc>,
    reminder: NewMeetingReminder,
) -> Result<MeetingReminder, MeetingError> {
    let recipient_email = reminder.recipient_email.map(normalize_email).transpose()?;
    if reminder.recipient_user_id.is_none() && recipient_email.is_none() {
        return Err(MeetingError::InvalidInput(
            "meeting reminder requires recipient_user_id or recipient_email",
        ));
    }
    if reminder.offset_minutes < 0 {
        return Err(MeetingError::InvalidInput(
            "meeting reminder offset_minutes must be greater than or equal to 0",
        ));
    }

    Ok(MeetingReminder {
        id: ids::new_uuid_v7(),
        meeting_id,
        recipient_user_id: reminder.recipient_user_id,
        recipient_email,
        channel: normalize_reminder_channel(reminder.channel)?,
        offset_minutes: reminder.offset_minutes,
        scheduled_for: format_time(
            starts_at - Duration::minutes(i64::from(reminder.offset_minutes)),
        ),
        status: "pending".to_owned(),
    })
}

fn normalize_attendee_role(role: Option<String>) -> Result<String, MeetingError> {
    match role
        .unwrap_or_else(|| "required".to_owned())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "host" => Ok("host".to_owned()),
        "required" => Ok("required".to_owned()),
        "optional" => Ok("optional".to_owned()),
        _ => Err(MeetingError::InvalidInput(
            "meeting attendee role must be host, required, or optional",
        )),
    }
}

fn normalize_reminder_channel(channel: String) -> Result<String, MeetingError> {
    match channel.trim().to_ascii_lowercase().as_str() {
        "in_app" => Ok("in_app".to_owned()),
        "push" => Ok("push".to_owned()),
        "email" => Ok("email".to_owned()),
        _ => Err(MeetingError::InvalidInput(
            "meeting reminder channel must be in_app, push, or email",
        )),
    }
}

fn normalize_email(email: String) -> Result<String, MeetingError> {
    let email = email.trim().to_ascii_lowercase();
    if email.contains('@') && email.len() <= 254 {
        Ok(email)
    } else {
        Err(MeetingError::InvalidInput(
            "meeting email must be a valid email address",
        ))
    }
}

fn parse_time(value: String, message: &'static str) -> Result<DateTime<Utc>, MeetingError> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| MeetingError::InvalidInput(message))
}

fn validate_schedule(starts_at: DateTime<Utc>, ends_at: DateTime<Utc>) -> Result<(), MeetingError> {
    if ends_at > starts_at {
        Ok(())
    } else {
        Err(MeetingError::InvalidInput(
            "meeting end time must be after start time",
        ))
    }
}

fn format_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}
