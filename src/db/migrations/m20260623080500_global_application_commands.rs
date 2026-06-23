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
                DROP INDEX IF EXISTS idx_application_commands_unique_name;

                ALTER TABLE application_commands
                    ALTER COLUMN space_id DROP NOT NULL;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_application_commands_unique_space_name
                    ON application_commands (application_id, space_id, name)
                    WHERE status = 'active' AND space_id IS NOT NULL;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_application_commands_unique_global_name
                    ON application_commands (application_id, name)
                    WHERE status = 'active' AND space_id IS NULL;
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
                DELETE FROM application_commands
                WHERE space_id IS NULL;

                DROP INDEX IF EXISTS idx_application_commands_unique_global_name;
                DROP INDEX IF EXISTS idx_application_commands_unique_space_name;

                ALTER TABLE application_commands
                    ALTER COLUMN space_id SET NOT NULL;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_application_commands_unique_name
                    ON application_commands (application_id, space_id, name)
                    WHERE status = 'active';
                "#,
            )
            .await?;

        Ok(())
    }
}
