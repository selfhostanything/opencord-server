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
                ALTER TABLE command_interactions
                    DROP CONSTRAINT IF EXISTS command_interactions_status_check;

                ALTER TABLE command_interactions
                    ADD CONSTRAINT command_interactions_status_check
                    CHECK (status IN ('pending', 'deferred', 'responded', 'expired'));
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
                UPDATE command_interactions
                SET status = 'responded'
                WHERE status = 'deferred';

                ALTER TABLE command_interactions
                    DROP CONSTRAINT IF EXISTS command_interactions_status_check;

                ALTER TABLE command_interactions
                    ADD CONSTRAINT command_interactions_status_check
                    CHECK (status IN ('pending', 'responded', 'expired'));
                "#,
            )
            .await?;

        Ok(())
    }
}
