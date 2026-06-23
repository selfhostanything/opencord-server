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
                ALTER TABLE connected_calendar_accounts
                    DROP CONSTRAINT IF EXISTS connected_calendar_accounts_provider_check;
                ALTER TABLE connected_calendar_accounts
                    ADD CONSTRAINT connected_calendar_accounts_provider_check
                    CHECK (provider IN ('google', 'microsoft', 'caldav'));

                ALTER TABLE calendar_event_syncs
                    DROP CONSTRAINT IF EXISTS calendar_event_syncs_provider_check;
                ALTER TABLE calendar_event_syncs
                    ADD CONSTRAINT calendar_event_syncs_provider_check
                    CHECK (provider IN ('google', 'microsoft', 'caldav'));
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
                ALTER TABLE calendar_event_syncs
                    DROP CONSTRAINT IF EXISTS calendar_event_syncs_provider_check;
                ALTER TABLE calendar_event_syncs
                    ADD CONSTRAINT calendar_event_syncs_provider_check
                    CHECK (provider IN ('google', 'microsoft'));

                ALTER TABLE connected_calendar_accounts
                    DROP CONSTRAINT IF EXISTS connected_calendar_accounts_provider_check;
                ALTER TABLE connected_calendar_accounts
                    ADD CONSTRAINT connected_calendar_accounts_provider_check
                    CHECK (provider IN ('google', 'microsoft'));
                "#,
            )
            .await?;

        Ok(())
    }
}
