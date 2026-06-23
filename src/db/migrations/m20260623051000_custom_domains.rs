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
                CREATE TABLE IF NOT EXISTS organization_custom_domains (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    hostname text NOT NULL UNIQUE,
                    verification_token text NOT NULL,
                    status text NOT NULL DEFAULT 'pending_verification',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now(),
                    verified_at timestamptz NULL
                );

                CREATE INDEX IF NOT EXISTS idx_organization_custom_domains_org
                    ON organization_custom_domains (organization_id, hostname);

                CREATE INDEX IF NOT EXISTS idx_organization_custom_domains_active_hostname
                    ON organization_custom_domains (hostname)
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
                DROP INDEX IF EXISTS idx_organization_custom_domains_active_hostname;
                DROP INDEX IF EXISTS idx_organization_custom_domains_org;
                DROP TABLE IF EXISTS organization_custom_domains;
                "#,
            )
            .await?;

        Ok(())
    }
}
