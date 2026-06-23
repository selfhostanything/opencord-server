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
                CREATE TABLE IF NOT EXISTS push_tokens (
                    id uuid PRIMARY KEY,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    platform text NOT NULL,
                    token text NOT NULL,
                    token_hash text NOT NULL,
                    token_last_four text NOT NULL,
                    device_name text,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CONSTRAINT push_tokens_platform_check
                        CHECK (platform IN ('ios', 'android', 'web', 'desktop')),
                    CONSTRAINT push_tokens_token_hash_unique
                        UNIQUE (user_id, platform, token_hash)
                );

                CREATE INDEX IF NOT EXISTS idx_push_tokens_user_created
                    ON push_tokens (user_id, created_at, id);
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
                DROP INDEX IF EXISTS idx_push_tokens_user_created;
                DROP TABLE IF EXISTS push_tokens;
                "#,
            )
            .await?;

        Ok(())
    }
}
