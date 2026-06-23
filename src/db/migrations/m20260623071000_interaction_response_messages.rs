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
                    ADD COLUMN IF NOT EXISTS response_message_id uuid NULL REFERENCES messages(id) ON DELETE SET NULL;

                CREATE INDEX IF NOT EXISTS idx_command_interactions_response_message
                    ON command_interactions (response_message_id)
                    WHERE response_message_id IS NOT NULL;
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
                DROP INDEX IF EXISTS idx_command_interactions_response_message;

                ALTER TABLE command_interactions
                    DROP COLUMN IF EXISTS response_message_id;
                "#,
            )
            .await?;

        Ok(())
    }
}
