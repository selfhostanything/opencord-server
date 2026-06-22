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
                CREATE TABLE IF NOT EXISTS users (
                    id uuid PRIMARY KEY,
                    email text NOT NULL UNIQUE,
                    display_name text NOT NULL,
                    password_hash text NOT NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE TABLE IF NOT EXISTS user_sessions (
                    id uuid PRIMARY KEY,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    token_hash text NOT NULL UNIQUE,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    revoked_at timestamptz NULL
                );

                CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id
                    ON user_sessions (user_id);

                CREATE INDEX IF NOT EXISTS idx_user_sessions_active_token
                    ON user_sessions (token_hash)
                    WHERE revoked_at IS NULL;
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
                DROP INDEX IF EXISTS idx_user_sessions_active_token;
                DROP INDEX IF EXISTS idx_user_sessions_user_id;
                DROP TABLE IF EXISTS user_sessions;
                DROP TABLE IF EXISTS users;
                "#,
            )
            .await?;

        Ok(())
    }
}
