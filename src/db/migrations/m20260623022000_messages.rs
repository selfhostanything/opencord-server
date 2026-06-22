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
                CREATE TABLE IF NOT EXISTS messages (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                    author_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    content text NOT NULL,
                    content_format text NOT NULL DEFAULT 'plain',
                    reply_to_message_id uuid NULL REFERENCES messages(id) ON DELETE SET NULL,
                    edited_at timestamptz NULL,
                    deleted_at timestamptz NULL,
                    created_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_messages_organization_id
                    ON messages (organization_id);

                CREATE INDEX IF NOT EXISTS idx_messages_channel_created
                    ON messages (channel_id, created_at, id)
                    WHERE deleted_at IS NULL;

                CREATE INDEX IF NOT EXISTS idx_messages_author_user_id
                    ON messages (author_user_id);
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
                DROP INDEX IF EXISTS idx_messages_author_user_id;
                DROP INDEX IF EXISTS idx_messages_channel_created;
                DROP INDEX IF EXISTS idx_messages_organization_id;
                DROP TABLE IF EXISTS messages;
                "#,
            )
            .await?;

        Ok(())
    }
}
