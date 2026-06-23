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
                CREATE TABLE IF NOT EXISTS compat_gateway_replay_events (
                    session_id text NOT NULL REFERENCES compat_gateway_sessions(session_id) ON DELETE CASCADE,
                    sequence bigint NOT NULL,
                    event_type text NOT NULL,
                    payload jsonb NOT NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    CONSTRAINT compat_gateway_replay_events_sequence_check
                        CHECK (sequence > 0),
                    PRIMARY KEY (session_id, sequence)
                );

                CREATE INDEX IF NOT EXISTS idx_compat_gateway_replay_events_created_at
                    ON compat_gateway_replay_events (created_at);
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
                DROP INDEX IF EXISTS idx_compat_gateway_replay_events_created_at;
                DROP TABLE IF EXISTS compat_gateway_replay_events;
                "#,
            )
            .await?;

        Ok(())
    }
}
