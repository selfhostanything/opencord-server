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
                CREATE TABLE IF NOT EXISTS billing_subscriptions (
                    id uuid PRIMARY KEY,
                    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                    provider text NOT NULL
                        CHECK (provider IN ('stripe')),
                    event_type text NOT NULL,
                    external_customer_id text NOT NULL,
                    external_subscription_id text NOT NULL,
                    plan text NOT NULL
                        CHECK (plan IN ('free', 'team', 'business', 'enterprise')),
                    status text NOT NULL
                        CHECK (status IN ('active', 'trialing', 'past_due', 'cancelled', 'unpaid')),
                    current_period_end timestamptz NULL,
                    created_at timestamptz NOT NULL DEFAULT now(),
                    updated_at timestamptz NOT NULL DEFAULT now()
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_subscriptions_organization
                    ON billing_subscriptions (organization_id);

                CREATE INDEX IF NOT EXISTS idx_billing_subscriptions_provider_customer
                    ON billing_subscriptions (provider, external_customer_id);
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
                DROP INDEX IF EXISTS idx_billing_subscriptions_provider_customer;
                DROP INDEX IF EXISTS idx_billing_subscriptions_organization;
                DROP TABLE IF EXISTS billing_subscriptions;
                "#,
            )
            .await?;

        Ok(())
    }
}
