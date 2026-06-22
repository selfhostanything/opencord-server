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
                CREATE TABLE IF NOT EXISTS roles (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    name text NOT NULL,
                    color text NULL,
                    position integer NOT NULL DEFAULT 0,
                    permissions_bitset bigint NOT NULL DEFAULT 0,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_roles_space_lower_name
                    ON roles (space_id, lower(name));

                CREATE INDEX IF NOT EXISTS idx_roles_space_position
                    ON roles (space_id, position, name);

                CREATE TABLE IF NOT EXISTS role_assignments (
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    role_id uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    assigned_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    PRIMARY KEY (role_id, user_id)
                );

                CREATE INDEX IF NOT EXISTS idx_role_assignments_space_user
                    ON role_assignments (space_id, user_id);

                CREATE TABLE IF NOT EXISTS channel_permission_overrides (
                    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                    target_kind text NOT NULL CHECK (target_kind IN ('role', 'member')),
                    target_id uuid NOT NULL,
                    allow_bitset bigint NOT NULL DEFAULT 0,
                    deny_bitset bigint NOT NULL DEFAULT 0,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    PRIMARY KEY (channel_id, target_kind, target_id)
                );

                CREATE INDEX IF NOT EXISTS idx_channel_permission_overrides_target
                    ON channel_permission_overrides (target_kind, target_id);
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
                DROP INDEX IF EXISTS idx_channel_permission_overrides_target;
                DROP TABLE IF EXISTS channel_permission_overrides;
                DROP INDEX IF EXISTS idx_role_assignments_space_user;
                DROP TABLE IF EXISTS role_assignments;
                DROP INDEX IF EXISTS idx_roles_space_position;
                DROP INDEX IF EXISTS idx_roles_space_lower_name;
                DROP TABLE IF EXISTS roles;
                "#,
            )
            .await?;

        Ok(())
    }
}
