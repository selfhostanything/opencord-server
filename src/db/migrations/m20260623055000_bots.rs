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
                CREATE TABLE IF NOT EXISTS bot_applications (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    bot_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    created_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    name text NOT NULL,
                    description text NULL,
                    status text NOT NULL DEFAULT 'active',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CONSTRAINT bot_applications_status_check
                        CHECK (status IN ('active', 'disabled'))
                );

                CREATE TABLE IF NOT EXISTS bot_tokens (
                    id uuid PRIMARY KEY,
                    application_id uuid NOT NULL REFERENCES bot_applications(id) ON DELETE CASCADE,
                    token_hash text NOT NULL UNIQUE,
                    token_last_four text NOT NULL,
                    created_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    active boolean NOT NULL DEFAULT true,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_bot_applications_organization
                    ON bot_applications (organization_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_bot_applications_bot_user
                    ON bot_applications (bot_user_id);

                CREATE INDEX IF NOT EXISTS idx_bot_tokens_application
                    ON bot_tokens (application_id, created_at, id);
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
                DROP INDEX IF EXISTS idx_bot_tokens_application;
                DROP INDEX IF EXISTS idx_bot_applications_bot_user;
                DROP INDEX IF EXISTS idx_bot_applications_organization;
                DROP TABLE IF EXISTS bot_tokens;
                DROP TABLE IF EXISTS bot_applications;
                "#,
            )
            .await?;

        Ok(())
    }
}
