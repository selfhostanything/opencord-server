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
                    ADD COLUMN IF NOT EXISTS mention_user_ids jsonb NOT NULL DEFAULT '[]'::jsonb,
                    ADD COLUMN IF NOT EXISTS mention_role_ids jsonb NOT NULL DEFAULT '[]'::jsonb,
                    ADD COLUMN IF NOT EXISTS mention_everyone boolean NOT NULL DEFAULT false;
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
                    DROP COLUMN IF EXISTS mention_everyone,
                    DROP COLUMN IF EXISTS mention_role_ids,
                    DROP COLUMN IF EXISTS mention_user_ids;
                "#,
            )
            .await?;

        Ok(())
    }
}
