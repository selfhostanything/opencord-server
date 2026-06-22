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
                CREATE TABLE IF NOT EXISTS attachments (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                    message_id uuid NULL REFERENCES messages(id) ON DELETE SET NULL,
                    uploader_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    file_name text NOT NULL,
                    content_type text NOT NULL,
                    size_bytes bigint NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 10485760),
                    status text NOT NULL CHECK (status IN ('pending', 'uploaded', 'linked')),
                    content bytea NULL,
                    uploaded_at timestamptz NULL,
                    created_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_attachments_organization_id
                    ON attachments (organization_id);

                CREATE INDEX IF NOT EXISTS idx_attachments_channel_id
                    ON attachments (channel_id);

                CREATE INDEX IF NOT EXISTS idx_attachments_message_id
                    ON attachments (message_id)
                    WHERE message_id IS NOT NULL;

                CREATE INDEX IF NOT EXISTS idx_attachments_uploader_pending
                    ON attachments (uploader_user_id, created_at)
                    WHERE message_id IS NULL;
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
                DROP INDEX IF EXISTS idx_attachments_uploader_pending;
                DROP INDEX IF EXISTS idx_attachments_message_id;
                DROP INDEX IF EXISTS idx_attachments_channel_id;
                DROP INDEX IF EXISTS idx_attachments_organization_id;
                DROP TABLE IF EXISTS attachments;
                "#,
            )
            .await?;

        Ok(())
    }
}
