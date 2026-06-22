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
                CREATE TABLE IF NOT EXISTS spaces (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    name text NOT NULL,
                    slug text NOT NULL,
                    icon_object_key text NULL,
                    owner_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    archived_at timestamptz NULL,
                    UNIQUE (organization_id, slug)
                );

                CREATE INDEX IF NOT EXISTS idx_spaces_organization_id
                    ON spaces (organization_id);

                CREATE TABLE IF NOT EXISTS space_members (
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    role text NOT NULL,
                    status text NOT NULL DEFAULT 'active',
                    nickname text NULL,
                    joined_at timestamptz NOT NULL DEFAULT now(),
                    muted_until timestamptz NULL,
                    last_read_at timestamptz NULL,
                    PRIMARY KEY (space_id, user_id)
                );

                CREATE INDEX IF NOT EXISTS idx_space_members_user_id
                    ON space_members (user_id);

                CREATE INDEX IF NOT EXISTS idx_space_members_active_user
                    ON space_members (user_id, space_id)
                    WHERE status = 'active';
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
                DROP INDEX IF EXISTS idx_space_members_active_user;
                DROP INDEX IF EXISTS idx_space_members_user_id;
                DROP TABLE IF EXISTS space_members;
                DROP INDEX IF EXISTS idx_spaces_organization_id;
                DROP TABLE IF EXISTS spaces;
                "#,
            )
            .await?;

        Ok(())
    }
}
