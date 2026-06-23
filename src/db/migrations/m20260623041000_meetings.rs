use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TABLE IF NOT EXISTS meetings (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NULL REFERENCES spaces(id) ON DELETE SET NULL,
                    channel_id uuid NULL REFERENCES channels(id) ON DELETE SET NULL,
                    created_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    title text NOT NULL,
                    description text NULL,
                    status text NOT NULL DEFAULT 'scheduled'
                        CHECK (status IN ('scheduled', 'cancelled')),
                    starts_at timestamptz NOT NULL,
                    ends_at timestamptz NOT NULL,
                    timezone text NOT NULL DEFAULT 'UTC',
                    join_slug text NOT NULL,
                    cancelled_at timestamptz NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CHECK (ends_at > starts_at)
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_meetings_join_slug
                    ON meetings (join_slug);

                CREATE INDEX IF NOT EXISTS idx_meetings_organization_start
                    ON meetings (organization_id, starts_at, id)
                    WHERE status <> 'cancelled';

                CREATE INDEX IF NOT EXISTS idx_meetings_space_start
                    ON meetings (space_id, starts_at, id)
                    WHERE space_id IS NOT NULL AND status <> 'cancelled';

                CREATE INDEX IF NOT EXISTS idx_meetings_channel_start
                    ON meetings (channel_id, starts_at, id)
                    WHERE channel_id IS NOT NULL AND status <> 'cancelled';

                CREATE TABLE IF NOT EXISTS meeting_attendees (
                    id uuid PRIMARY KEY,
                    meeting_id uuid NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                    user_id uuid NULL REFERENCES users(id) ON DELETE CASCADE,
                    email text NULL,
                    display_name text NULL,
                    role text NOT NULL DEFAULT 'required'
                        CHECK (role IN ('host', 'required', 'optional')),
                    response_status text NOT NULL DEFAULT 'needs_action'
                        CHECK (response_status IN ('needs_action', 'accepted', 'declined', 'tentative')),
                    invited_by_user_id uuid NULL REFERENCES users(id) ON DELETE SET NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CHECK (user_id IS NOT NULL OR email IS NOT NULL)
                );

                CREATE INDEX IF NOT EXISTS idx_meeting_attendees_meeting
                    ON meeting_attendees (meeting_id);

                CREATE INDEX IF NOT EXISTS idx_meeting_attendees_user
                    ON meeting_attendees (user_id, meeting_id)
                    WHERE user_id IS NOT NULL;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_meeting_attendees_meeting_user
                    ON meeting_attendees (meeting_id, user_id)
                    WHERE user_id IS NOT NULL;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_meeting_attendees_meeting_email
                    ON meeting_attendees (meeting_id, lower(email))
                    WHERE email IS NOT NULL;

                CREATE TABLE IF NOT EXISTS meeting_reminders (
                    id uuid PRIMARY KEY,
                    meeting_id uuid NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                    recipient_user_id uuid NULL REFERENCES users(id) ON DELETE CASCADE,
                    recipient_email text NULL,
                    channel text NOT NULL
                        CHECK (channel IN ('in_app', 'push', 'email')),
                    offset_minutes integer NOT NULL,
                    scheduled_for timestamptz NOT NULL,
                    status text NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'sent', 'failed', 'cancelled')),
                    sent_at timestamptz NULL,
                    failed_at timestamptz NULL,
                    failure_reason text NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CHECK (recipient_user_id IS NOT NULL OR recipient_email IS NOT NULL),
                    CHECK (offset_minutes >= 0)
                );

                CREATE INDEX IF NOT EXISTS idx_meeting_reminders_meeting
                    ON meeting_reminders (meeting_id);

                CREATE INDEX IF NOT EXISTS idx_meeting_reminders_recipient
                    ON meeting_reminders (recipient_user_id, scheduled_for)
                    WHERE recipient_user_id IS NOT NULL;

                CREATE INDEX IF NOT EXISTS idx_meeting_reminders_due
                    ON meeting_reminders (scheduled_for, id)
                    WHERE status = 'pending';
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP INDEX IF EXISTS idx_meeting_reminders_due;
                DROP INDEX IF EXISTS idx_meeting_reminders_recipient;
                DROP INDEX IF EXISTS idx_meeting_reminders_meeting;
                DROP TABLE IF EXISTS meeting_reminders;
                DROP INDEX IF EXISTS idx_meeting_attendees_meeting_email;
                DROP INDEX IF EXISTS idx_meeting_attendees_meeting_user;
                DROP INDEX IF EXISTS idx_meeting_attendees_user;
                DROP INDEX IF EXISTS idx_meeting_attendees_meeting;
                DROP TABLE IF EXISTS meeting_attendees;
                DROP INDEX IF EXISTS idx_meetings_channel_start;
                DROP INDEX IF EXISTS idx_meetings_space_start;
                DROP INDEX IF EXISTS idx_meetings_organization_start;
                DROP INDEX IF EXISTS idx_meetings_join_slug;
                DROP TABLE IF EXISTS meetings;
                "#,
            )
            .await?;

        Ok(())
    }
}
