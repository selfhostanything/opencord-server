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
                CREATE TABLE IF NOT EXISTS interaction_followup_messages (
                    interaction_id uuid NOT NULL REFERENCES command_interactions(id) ON DELETE CASCADE,
                    message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    PRIMARY KEY (interaction_id, message_id),
                    CONSTRAINT interaction_followup_messages_unique_message UNIQUE (message_id)
                );

                CREATE INDEX IF NOT EXISTS idx_interaction_followup_messages_interaction
                    ON interaction_followup_messages (interaction_id, created_at, message_id);
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
                DROP INDEX IF EXISTS idx_interaction_followup_messages_interaction;
                DROP TABLE IF EXISTS interaction_followup_messages;
                "#,
            )
            .await?;

        Ok(())
    }
}
