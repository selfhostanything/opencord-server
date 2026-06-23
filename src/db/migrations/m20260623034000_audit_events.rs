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
                CREATE TABLE IF NOT EXISTS audit_events (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    space_id uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                    actor_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
                    action text NOT NULL,
                    target_type text NOT NULL,
                    target_id uuid NOT NULL,
                    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
                    created_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_audit_events_space_created
                    ON audit_events (space_id, created_at, id);

                CREATE INDEX IF NOT EXISTS idx_audit_events_organization_id
                    ON audit_events (organization_id);

                CREATE INDEX IF NOT EXISTS idx_audit_events_actor_user_id
                    ON audit_events (actor_user_id);
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
                DROP INDEX IF EXISTS idx_audit_events_actor_user_id;
                DROP INDEX IF EXISTS idx_audit_events_organization_id;
                DROP INDEX IF EXISTS idx_audit_events_space_created;
                DROP TABLE IF EXISTS audit_events;
                "#,
            )
            .await?;

        Ok(())
    }
}
