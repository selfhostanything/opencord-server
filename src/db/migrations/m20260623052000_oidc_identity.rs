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
                CREATE TABLE IF NOT EXISTS organization_oidc_providers (
                    organization_id uuid PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
                    issuer text NOT NULL,
                    authorization_endpoint text NOT NULL,
                    token_endpoint text NOT NULL,
                    jwks_uri text NOT NULL,
                    client_id text NOT NULL,
                    client_secret text NOT NULL,
                    allowed_domains_json text NOT NULL,
                    require_sso boolean NOT NULL DEFAULT false,
                    auto_join_role text NOT NULL DEFAULT 'member',
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE INDEX IF NOT EXISTS idx_organization_oidc_providers_issuer
                    ON organization_oidc_providers (issuer);

                CREATE TABLE IF NOT EXISTS user_oidc_identities (
                    id uuid PRIMARY KEY,
                    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    issuer text NOT NULL,
                    subject text NOT NULL,
                    email text NOT NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    UNIQUE (issuer, subject),
                    UNIQUE (user_id, organization_id, issuer)
                );

                CREATE INDEX IF NOT EXISTS idx_user_oidc_identities_user
                    ON user_oidc_identities (user_id);
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
                DROP INDEX IF EXISTS idx_user_oidc_identities_user;
                DROP TABLE IF EXISTS user_oidc_identities;
                DROP INDEX IF EXISTS idx_organization_oidc_providers_issuer;
                DROP TABLE IF EXISTS organization_oidc_providers;
                "#,
            )
            .await?;

        Ok(())
    }
}
