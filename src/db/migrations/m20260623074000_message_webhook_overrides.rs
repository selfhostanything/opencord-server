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
                ALTER TABLE messages
                    ADD COLUMN IF NOT EXISTS webhook_username text NULL,
                    ADD COLUMN IF NOT EXISTS webhook_avatar_url text NULL;
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
                ALTER TABLE messages
                    DROP COLUMN IF EXISTS webhook_avatar_url,
                    DROP COLUMN IF EXISTS webhook_username;
                "#,
            )
            .await?;

        Ok(())
    }
}
