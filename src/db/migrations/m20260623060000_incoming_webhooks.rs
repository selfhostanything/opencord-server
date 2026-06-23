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
                CREATE TABLE IF NOT EXISTS incoming_webhooks (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                    bot_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    created_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    name text NOT NULL,
                    token_hash text NOT NULL UNIQUE,
                    token_last_four text NOT NULL,
                    status text NOT NULL DEFAULT 'active',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CONSTRAINT incoming_webhooks_status_check
                        CHECK (status IN ('active', 'disabled'))
                );

                CREATE INDEX IF NOT EXISTS idx_incoming_webhooks_channel
                    ON incoming_webhooks (channel_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_incoming_webhooks_organization
                    ON incoming_webhooks (organization_id, created_at, id);
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
                DROP INDEX IF EXISTS idx_incoming_webhooks_organization;
                DROP INDEX IF EXISTS idx_incoming_webhooks_channel;
                DROP TABLE IF EXISTS incoming_webhooks;
                "#,
            )
            .await?;

        Ok(())
    }
}
