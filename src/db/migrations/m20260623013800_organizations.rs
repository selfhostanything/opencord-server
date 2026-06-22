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
                CREATE TABLE IF NOT EXISTS organizations (
                    id uuid PRIMARY KEY,
                    slug text NOT NULL UNIQUE,
                    name text NOT NULL,
                    plan text NOT NULL DEFAULT 'free',
                    deployment_mode text NOT NULL DEFAULT 'self_hosted',
                    primary_region text NOT NULL DEFAULT 'local',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    suspended_at timestamptz NULL
                );

                CREATE TABLE IF NOT EXISTS organization_members (
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    role text NOT NULL,
                    status text NOT NULL DEFAULT 'active',
                    joined_at timestamptz NOT NULL DEFAULT now(),
                    last_active_at timestamptz NULL,
                    PRIMARY KEY (organization_id, user_id)
                );

                CREATE INDEX IF NOT EXISTS idx_organization_members_user_id
                    ON organization_members (user_id);

                CREATE INDEX IF NOT EXISTS idx_organization_members_active_user
                    ON organization_members (user_id, organization_id)
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
                DROP INDEX IF EXISTS idx_organization_members_active_user;
                DROP INDEX IF EXISTS idx_organization_members_user_id;
                DROP TABLE IF EXISTS organization_members;
                DROP TABLE IF EXISTS organizations;
                "#,
            )
            .await?;

        Ok(())
    }
}
