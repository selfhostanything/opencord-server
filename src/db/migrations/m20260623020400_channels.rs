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
                CREATE TABLE IF NOT EXISTS channels (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    parent_category_id uuid NULL,
                    kind text NOT NULL,
                    name text NOT NULL,
                    slug text NOT NULL,
                    position integer NOT NULL DEFAULT 0,
                    topic text NULL,
                    is_private boolean NOT NULL DEFAULT false,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    archived_at timestamptz NULL,
                    UNIQUE (space_id, slug)
                );

                CREATE INDEX IF NOT EXISTS idx_channels_organization_id
                    ON channels (organization_id);

                CREATE INDEX IF NOT EXISTS idx_channels_space_position
                    ON channels (space_id, position, name)
                    WHERE archived_at IS NULL;
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
                DROP INDEX IF EXISTS idx_channels_space_position;
                DROP INDEX IF EXISTS idx_channels_organization_id;
                DROP TABLE IF EXISTS channels;
                "#,
            )
            .await?;

        Ok(())
    }
}
