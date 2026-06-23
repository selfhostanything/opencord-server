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
                CREATE TABLE IF NOT EXISTS compat_gateway_sessions (
                    session_id text PRIMARY KEY,
                    application_id uuid NOT NULL REFERENCES bot_applications(id) ON DELETE CASCADE,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    bot_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    sequence bigint NOT NULL DEFAULT 0,
                    intents bigint NOT NULL DEFAULT 0,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    expires_at timestamptz NOT NULL DEFAULT (now() + interval '24 hours'),
                    CONSTRAINT compat_gateway_sessions_sequence_check
                        CHECK (sequence >= 0),
                    CONSTRAINT compat_gateway_sessions_intents_check
                        CHECK (intents >= 0)
                );

                CREATE INDEX IF NOT EXISTS idx_compat_gateway_sessions_application
                    ON compat_gateway_sessions (application_id, updated_at DESC);

                CREATE INDEX IF NOT EXISTS idx_compat_gateway_sessions_organization
                    ON compat_gateway_sessions (organization_id, updated_at DESC);

                CREATE INDEX IF NOT EXISTS idx_compat_gateway_sessions_expires_at
                    ON compat_gateway_sessions (expires_at);
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
                DROP INDEX IF EXISTS idx_compat_gateway_sessions_expires_at;
                DROP INDEX IF EXISTS idx_compat_gateway_sessions_organization;
                DROP INDEX IF EXISTS idx_compat_gateway_sessions_application;
                DROP TABLE IF EXISTS compat_gateway_sessions;
                "#,
            )
            .await?;

        Ok(())
    }
}
