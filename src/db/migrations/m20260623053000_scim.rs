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
                CREATE TABLE IF NOT EXISTS organization_scim_tokens (
                    organization_id uuid PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
                    token_hash text NOT NULL UNIQUE,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE TABLE IF NOT EXISTS scim_users (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    external_id text NOT NULL,
                    user_name text NOT NULL,
                    display_name text NOT NULL,
                    active boolean NOT NULL DEFAULT true,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    UNIQUE (organization_id, external_id)
                );

                CREATE INDEX IF NOT EXISTS idx_scim_users_user
                    ON scim_users (user_id);
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
                DROP INDEX IF EXISTS idx_scim_users_user;
                DROP TABLE IF EXISTS scim_users;
                DROP TABLE IF EXISTS organization_scim_tokens;
                "#,
            )
            .await?;

        Ok(())
    }
}
