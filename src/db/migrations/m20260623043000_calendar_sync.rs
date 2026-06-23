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
                CREATE TABLE IF NOT EXISTS connected_calendar_accounts (
                    id uuid PRIMARY KEY,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    provider text NOT NULL
                        CHECK (provider IN ('google')),
                    external_account_id text NOT NULL,
                    calendar_id text NOT NULL DEFAULT 'primary',
                    access_token_ciphertext text NOT NULL,
                    refresh_token_ciphertext text NULL,
                    token_last_four text NOT NULL,
                    sync_enabled boolean NOT NULL DEFAULT true,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_connected_calendar_accounts_user_provider
                    ON connected_calendar_accounts (user_id, provider);

                CREATE INDEX IF NOT EXISTS idx_connected_calendar_accounts_provider_external
                    ON connected_calendar_accounts (provider, external_account_id);

                CREATE TABLE IF NOT EXISTS calendar_event_syncs (
                    id uuid PRIMARY KEY,
                    meeting_id uuid NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                    account_id uuid NOT NULL REFERENCES connected_calendar_accounts(id) ON DELETE CASCADE,
                    provider text NOT NULL
                        CHECK (provider IN ('google')),
                    provider_event_id text NOT NULL,
                    provider_event_url text NULL,
                    calendar_id text NOT NULL,
                    status text NOT NULL
                        CHECK (status IN ('synced', 'failed')),
                    last_synced_at timestamptz NOT NULL,
                    failure_reason text NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_calendar_event_syncs_meeting_account_provider
                    ON calendar_event_syncs (meeting_id, account_id, provider);

                CREATE UNIQUE INDEX IF NOT EXISTS idx_calendar_event_syncs_account_provider_event
                    ON calendar_event_syncs (account_id, provider, provider_event_id);

                CREATE INDEX IF NOT EXISTS idx_calendar_event_syncs_meeting
                    ON calendar_event_syncs (meeting_id);
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
                DROP INDEX IF EXISTS idx_calendar_event_syncs_meeting;
                DROP INDEX IF EXISTS idx_calendar_event_syncs_account_provider_event;
                DROP INDEX IF EXISTS idx_calendar_event_syncs_meeting_account_provider;
                DROP TABLE IF EXISTS calendar_event_syncs;
                DROP INDEX IF EXISTS idx_connected_calendar_accounts_provider_external;
                DROP INDEX IF EXISTS idx_connected_calendar_accounts_user_provider;
                DROP TABLE IF EXISTS connected_calendar_accounts;
                "#,
            )
            .await?;

        Ok(())
    }
}
