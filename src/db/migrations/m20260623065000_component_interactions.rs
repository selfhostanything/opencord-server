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
                    ADD COLUMN IF NOT EXISTS interaction_type integer NOT NULL DEFAULT 2,
                    ADD COLUMN IF NOT EXISTS message_id uuid NULL REFERENCES messages(id) ON DELETE CASCADE,
                    ADD COLUMN IF NOT EXISTS custom_id text NULL,
                    ADD COLUMN IF NOT EXISTS component_type integer NULL;

                ALTER TABLE command_interactions
                    ALTER COLUMN command_id DROP NOT NULL;

                ALTER TABLE command_interactions
                    DROP CONSTRAINT IF EXISTS command_interactions_type_check;

                ALTER TABLE command_interactions
                    ADD CONSTRAINT command_interactions_type_check
                    CHECK (
                        (
                            interaction_type = 2
                            AND command_id IS NOT NULL
                            AND message_id IS NULL
                            AND custom_id IS NULL
                            AND component_type IS NULL
                        )
                        OR
                        (
                            interaction_type = 3
                            AND command_id IS NULL
                            AND message_id IS NOT NULL
                            AND custom_id IS NOT NULL
                            AND component_type IS NOT NULL
                        )
                    );

                CREATE INDEX IF NOT EXISTS idx_command_interactions_message
                    ON command_interactions (message_id, created_at, id)
                    WHERE message_id IS NOT NULL;

                CREATE INDEX IF NOT EXISTS idx_command_interactions_application_type
                    ON command_interactions (application_id, interaction_type, created_at, id);
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
                DROP INDEX IF EXISTS idx_command_interactions_application_type;
                DROP INDEX IF EXISTS idx_command_interactions_message;

                ALTER TABLE command_interactions
                    DROP CONSTRAINT IF EXISTS command_interactions_type_check;

                DELETE FROM command_interactions
                WHERE command_id IS NULL;

                ALTER TABLE command_interactions
                    ALTER COLUMN command_id SET NOT NULL,
                    DROP COLUMN IF EXISTS component_type,
                    DROP COLUMN IF EXISTS custom_id,
                    DROP COLUMN IF EXISTS message_id,
                    DROP COLUMN IF EXISTS interaction_type;
                "#,
            )
            .await?;

        Ok(())
    }
}
