use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};

use crate::domain::billing::{BillingError, BillingState, BillingStore};

#[derive(Clone)]
pub struct PostgresBillingStore {
    db: DatabaseConnection,
}

impl PostgresBillingStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl BillingStore for PostgresBillingStore {
    async fn upsert_state(&self, state: BillingState) -> Result<BillingState, BillingError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO billing_subscriptions (
                    id, organization_id, provider, event_type, external_customer_id,
                    external_subscription_id, plan, status, current_period_end, updated_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3, $4, $5,
                    $6, $7, $8, $9::timestamptz, $10::timestamptz
                )
                ON CONFLICT (organization_id)
                DO UPDATE SET
                    provider = EXCLUDED.provider,
                    event_type = EXCLUDED.event_type,
                    external_customer_id = EXCLUDED.external_customer_id,
                    external_subscription_id = EXCLUDED.external_subscription_id,
                    plan = EXCLUDED.plan,
                    status = EXCLUDED.status,
                    current_period_end = EXCLUDED.current_period_end,
                    updated_at = EXCLUDED.updated_at
                RETURNING id::text, organization_id::text, provider, event_type,
                          external_customer_id, external_subscription_id, plan, status,
                          CASE
                              WHEN current_period_end IS NULL THEN NULL
                              ELSE to_char(current_period_end AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                          END AS current_period_end,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                "#,
                state_values(&state),
            ))
            .await
            .map_err(|_| BillingError::StoreUnavailable)?;

        row.map(state_from_row)
            .transpose()?
            .ok_or(BillingError::StoreUnavailable)
    }
}

fn state_values(state: &BillingState) -> Vec<Value> {
    vec![
        Value::from(state.id.to_string()),
        Value::from(state.organization_id.to_string()),
        Value::from(state.provider.clone()),
        Value::from(state.event_type.clone()),
        Value::from(state.external_customer_id.clone()),
        Value::from(state.external_subscription_id.clone()),
        Value::from(state.plan.clone()),
        Value::from(state.status.clone()),
        Value::from(state.current_period_end.clone()),
        Value::from(state.updated_at.clone()),
    ]
}

fn state_from_row(row: sea_orm::QueryResult) -> Result<BillingState, BillingError> {
    Ok(BillingState {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| BillingError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| BillingError::StoreUnavailable)?,
        )?,
        provider: row
            .try_get::<String>("", "provider")
            .map_err(|_| BillingError::StoreUnavailable)?,
        event_type: row
            .try_get::<String>("", "event_type")
            .map_err(|_| BillingError::StoreUnavailable)?,
        external_customer_id: row
            .try_get::<String>("", "external_customer_id")
            .map_err(|_| BillingError::StoreUnavailable)?,
        external_subscription_id: row
            .try_get::<String>("", "external_subscription_id")
            .map_err(|_| BillingError::StoreUnavailable)?,
        plan: row
            .try_get::<String>("", "plan")
            .map_err(|_| BillingError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| BillingError::StoreUnavailable)?,
        current_period_end: row
            .try_get::<Option<String>>("", "current_period_end")
            .map_err(|_| BillingError::StoreUnavailable)?,
        updated_at: row
            .try_get::<String>("", "updated_at")
            .map_err(|_| BillingError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, BillingError> {
    uuid::Uuid::parse_str(value).map_err(|_| BillingError::StoreUnavailable)
}
