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
                CREATE TABLE IF NOT EXISTS organization_retention_policies (
                    organization_id uuid PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
                    messages_retain_days integer NULL CHECK (messages_retain_days BETWEEN 1 AND 3650),
                    files_retain_days integer NULL CHECK (files_retain_days BETWEEN 1 AND 3650),
                    audit_logs_retain_days integer NULL CHECK (audit_logs_retain_days BETWEEN 1 AND 3650),
                    deleted_message_purge_days integer NULL CHECK (deleted_message_purge_days BETWEEN 1 AND 3650),
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE TABLE IF NOT EXISTS retention_runs (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    dry_run boolean NOT NULL,
                    messages_purged bigint NOT NULL DEFAULT 0,
                    files_purged bigint NOT NULL DEFAULT 0,
                    audit_events_purged bigint NOT NULL DEFAULT 0,
                    ran_at timestamptz NOT NULL DEFAULT now(),
                    created_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_retention_runs_organization_ran_at
                    ON retention_runs (organization_id, ran_at, id);
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
                DROP INDEX IF EXISTS idx_retention_runs_organization_ran_at;
                DROP TABLE IF EXISTS retention_runs;
                DROP TABLE IF EXISTS organization_retention_policies;
                "#,
            )
            .await?;

        Ok(())
    }
}
