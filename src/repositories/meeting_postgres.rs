use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use crate::domain::meeting::{
    Meeting, MeetingAttendee, MeetingBundle, MeetingError, MeetingReminder, MeetingStore,
};

#[derive(Clone)]
pub struct PostgresMeetingStore {
    db: DatabaseConnection,
}

impl PostgresMeetingStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl MeetingStore for PostgresMeetingStore {
    async fn create_meeting(&self, bundle: MeetingBundle) -> Result<(), MeetingError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;

        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
            INSERT INTO meetings (
                id, organization_id, space_id, channel_id, created_by_user_id,
                title, description, status, starts_at, ends_at, timezone,
                join_slug, cancelled_at
            )
            VALUES (
                $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                $6, $7, $8, $9::timestamptz, $10::timestamptz, $11,
                $12, $13::timestamptz
            )
            "#,
            meeting_values(&bundle.meeting),
        ))
        .await
        .map_err(|_| MeetingError::StoreUnavailable)?;

        for attendee in bundle.attendees {
            txn.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO meeting_attendees (
                    id, meeting_id, user_id, email, display_name,
                    role, response_status
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4, $5,
                    $6, $7
                )
                "#,
                attendee_values(&attendee),
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;
        }

        for reminder in bundle.reminders {
            txn.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO meeting_reminders (
                    id, meeting_id, recipient_user_id, recipient_email,
                    channel, offset_minutes, scheduled_for, status
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4,
                    $5, $6, $7::timestamptz, $8
                )
                "#,
                reminder_values(&reminder),
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;
        }

        txn.commit()
            .await
            .map_err(|_| MeetingError::StoreUnavailable)
    }

    async fn list_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<MeetingBundle>, MeetingError> {
        let meetings = self
            .query_meetings(
                r#"
                WHERE organization_id = $1::uuid
                ORDER BY starts_at ASC, id ASC
                "#,
                vec![Value::from(organization_id.to_string())],
            )
            .await?;
        let mut bundles = Vec::with_capacity(meetings.len());

        for meeting in meetings {
            bundles.push(self.bundle_for_meeting(meeting).await?);
        }

        Ok(bundles)
    }

    async fn get_meeting(&self, meeting_id: Uuid) -> Result<Option<MeetingBundle>, MeetingError> {
        let meeting = self
            .query_meetings(
                r#"
                WHERE id = $1::uuid
                "#,
                vec![Value::from(meeting_id.to_string())],
            )
            .await?
            .into_iter()
            .next();

        match meeting {
            Some(meeting) => Ok(Some(self.bundle_for_meeting(meeting).await?)),
            None => Ok(None),
        }
    }

    async fn update_meeting(&self, meeting: Meeting) -> Result<MeetingBundle, MeetingError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE meetings
                SET title = $6,
                    description = $7,
                    status = $8,
                    starts_at = $9::timestamptz,
                    ends_at = $10::timestamptz,
                    timezone = $11,
                    join_slug = $12,
                    cancelled_at = $13::timestamptz,
                    updated_at = now()
                WHERE id = $1::uuid
                "#,
                meeting_values(&meeting),
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;

        if result.rows_affected() == 0 {
            return Err(MeetingError::NotFound);
        }

        self.get_meeting(meeting.id)
            .await?
            .ok_or(MeetingError::NotFound)
    }
}

impl PostgresMeetingStore {
    async fn bundle_for_meeting(&self, meeting: Meeting) -> Result<MeetingBundle, MeetingError> {
        let attendees = self.meeting_attendees(meeting.id).await?;
        let reminders = self.meeting_reminders(meeting.id).await?;

        Ok(MeetingBundle {
            meeting,
            attendees,
            reminders,
        })
    }

    async fn query_meetings(
        &self,
        where_clause: &str,
        values: Vec<Value>,
    ) -> Result<Vec<Meeting>, MeetingError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                meeting_select_sql(where_clause),
                values,
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;

        rows.into_iter()
            .map(meeting_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn meeting_attendees(
        &self,
        meeting_id: Uuid,
    ) -> Result<Vec<MeetingAttendee>, MeetingError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, meeting_id::text, user_id::text, email,
                       display_name, role, response_status
                FROM meeting_attendees
                WHERE meeting_id = $1::uuid
                ORDER BY created_at ASC, id ASC
                "#,
                vec![Value::from(meeting_id.to_string())],
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;

        rows.into_iter()
            .map(attendee_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn meeting_reminders(
        &self,
        meeting_id: Uuid,
    ) -> Result<Vec<MeetingReminder>, MeetingError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, meeting_id::text, recipient_user_id::text,
                       recipient_email, channel, offset_minutes,
                       to_char(scheduled_for AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS scheduled_for,
                       status
                FROM meeting_reminders
                WHERE meeting_id = $1::uuid
                ORDER BY scheduled_for ASC, id ASC
                "#,
                vec![Value::from(meeting_id.to_string())],
            ))
            .await
            .map_err(|_| MeetingError::StoreUnavailable)?;

        rows.into_iter()
            .map(reminder_from_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn meeting_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, organization_id::text, space_id::text, channel_id::text,
               created_by_user_id::text, title, description, status,
               to_char(starts_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS starts_at,
               to_char(ends_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS ends_at,
               timezone, join_slug,
               CASE
                   WHEN cancelled_at IS NULL THEN NULL
                   ELSE to_char(cancelled_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
               END AS cancelled_at
        FROM meetings
        {where_clause}
        "#
    )
}

fn meeting_from_row(row: sea_orm::QueryResult) -> Result<Meeting, MeetingError> {
    Ok(Meeting {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        space_id: optional_uuid(row.try_get::<Option<String>>("", "space_id"))?,
        channel_id: optional_uuid(row.try_get::<Option<String>>("", "channel_id"))?,
        created_by_user_id: parse_uuid(
            &row.try_get::<String>("", "created_by_user_id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        title: row
            .try_get::<String>("", "title")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        description: row
            .try_get::<Option<String>>("", "description")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        starts_at: row
            .try_get::<String>("", "starts_at")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        ends_at: row
            .try_get::<String>("", "ends_at")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        timezone: row
            .try_get::<String>("", "timezone")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        join_slug: row
            .try_get::<String>("", "join_slug")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        cancelled_at: row
            .try_get::<Option<String>>("", "cancelled_at")
            .map_err(|_| MeetingError::StoreUnavailable)?,
    })
}

fn attendee_from_row(row: sea_orm::QueryResult) -> Result<MeetingAttendee, MeetingError> {
    Ok(MeetingAttendee {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        meeting_id: parse_uuid(
            &row.try_get::<String>("", "meeting_id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        user_id: optional_uuid(row.try_get::<Option<String>>("", "user_id"))?,
        email: row
            .try_get::<Option<String>>("", "email")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        display_name: row
            .try_get::<Option<String>>("", "display_name")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        role: row
            .try_get::<String>("", "role")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        response_status: row
            .try_get::<String>("", "response_status")
            .map_err(|_| MeetingError::StoreUnavailable)?,
    })
}

fn reminder_from_row(row: sea_orm::QueryResult) -> Result<MeetingReminder, MeetingError> {
    Ok(MeetingReminder {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        meeting_id: parse_uuid(
            &row.try_get::<String>("", "meeting_id")
                .map_err(|_| MeetingError::StoreUnavailable)?,
        )?,
        recipient_user_id: optional_uuid(row.try_get::<Option<String>>("", "recipient_user_id"))?,
        recipient_email: row
            .try_get::<Option<String>>("", "recipient_email")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        channel: row
            .try_get::<String>("", "channel")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        offset_minutes: row
            .try_get::<i32>("", "offset_minutes")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        scheduled_for: row
            .try_get::<String>("", "scheduled_for")
            .map_err(|_| MeetingError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| MeetingError::StoreUnavailable)?,
    })
}

fn meeting_values(meeting: &Meeting) -> Vec<Value> {
    vec![
        Value::from(meeting.id.to_string()),
        Value::from(meeting.organization_id.to_string()),
        Value::from(meeting.space_id.map(|id| id.to_string())),
        Value::from(meeting.channel_id.map(|id| id.to_string())),
        Value::from(meeting.created_by_user_id.to_string()),
        Value::from(meeting.title.clone()),
        Value::from(meeting.description.clone()),
        Value::from(meeting.status.clone()),
        Value::from(meeting.starts_at.clone()),
        Value::from(meeting.ends_at.clone()),
        Value::from(meeting.timezone.clone()),
        Value::from(meeting.join_slug.clone()),
        Value::from(meeting.cancelled_at.clone()),
    ]
}

fn attendee_values(attendee: &MeetingAttendee) -> Vec<Value> {
    vec![
        Value::from(attendee.id.to_string()),
        Value::from(attendee.meeting_id.to_string()),
        Value::from(attendee.user_id.map(|id| id.to_string())),
        Value::from(attendee.email.clone()),
        Value::from(attendee.display_name.clone()),
        Value::from(attendee.role.clone()),
        Value::from(attendee.response_status.clone()),
    ]
}

fn reminder_values(reminder: &MeetingReminder) -> Vec<Value> {
    vec![
        Value::from(reminder.id.to_string()),
        Value::from(reminder.meeting_id.to_string()),
        Value::from(reminder.recipient_user_id.map(|id| id.to_string())),
        Value::from(reminder.recipient_email.clone()),
        Value::from(reminder.channel.clone()),
        Value::from(reminder.offset_minutes),
        Value::from(reminder.scheduled_for.clone()),
        Value::from(reminder.status.clone()),
    ]
}

fn optional_uuid(
    value: Result<Option<String>, sea_orm::DbErr>,
) -> Result<Option<Uuid>, MeetingError> {
    value
        .map_err(|_| MeetingError::StoreUnavailable)?
        .map(|id| parse_uuid(&id))
        .transpose()
}

fn parse_uuid(value: &str) -> Result<Uuid, MeetingError> {
    Uuid::parse_str(value).map_err(|_| MeetingError::StoreUnavailable)
}
