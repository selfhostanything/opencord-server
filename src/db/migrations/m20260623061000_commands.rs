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
                CREATE TABLE IF NOT EXISTS application_commands (
                    id uuid PRIMARY KEY,
                    application_id uuid NOT NULL REFERENCES bot_applications(id) ON DELETE CASCADE,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    created_by_bot_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    name text NOT NULL,
                    description text NOT NULL,
                    kind integer NOT NULL DEFAULT 1,
                    options jsonb NOT NULL DEFAULT '[]'::jsonb,
                    status text NOT NULL DEFAULT 'active',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    CONSTRAINT application_commands_kind_check
                        CHECK (kind = 1),
                    CONSTRAINT application_commands_status_check
                        CHECK (status IN ('active', 'disabled'))
                );

                CREATE TABLE IF NOT EXISTS command_interactions (
                    id uuid PRIMARY KEY,
                    application_id uuid NOT NULL REFERENCES bot_applications(id) ON DELETE CASCADE,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    channel_id uuid NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                    command_id uuid NOT NULL REFERENCES application_commands(id) ON DELETE CASCADE,
                    invoking_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    token_hash text NOT NULL UNIQUE,
                    token_last_four text NOT NULL,
                    status text NOT NULL DEFAULT 'pending',
                    options jsonb NOT NULL DEFAULT '[]'::jsonb,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    responded_at timestamptz NULL,
                    CONSTRAINT command_interactions_status_check
                        CHECK (status IN ('pending', 'responded', 'expired'))
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_application_commands_unique_name
                    ON application_commands (application_id, space_id, name)
                    WHERE status = 'active';

                CREATE INDEX IF NOT EXISTS idx_application_commands_space
                    ON application_commands (space_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_application_commands_application
                    ON application_commands (application_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_command_interactions_command
                    ON command_interactions (command_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_command_interactions_channel
                    ON command_interactions (channel_id, created_at, id);
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
                DROP INDEX IF EXISTS idx_command_interactions_channel;
                DROP INDEX IF EXISTS idx_command_interactions_command;
                DROP INDEX IF EXISTS idx_application_commands_application;
                DROP INDEX IF EXISTS idx_application_commands_space;
                DROP INDEX IF EXISTS idx_application_commands_unique_name;
                DROP TABLE IF EXISTS command_interactions;
                DROP TABLE IF EXISTS application_commands;
                "#,
            )
            .await?;

        Ok(())
    }
}
