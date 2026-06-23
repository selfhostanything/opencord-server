use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::webhook::{IncomingWebhook, IncomingWebhookStore, WebhookError};

#[derive(Clone)]
pub struct PostgresIncomingWebhookStore {
    db: DatabaseConnection,
}

impl PostgresIncomingWebhookStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl IncomingWebhookStore for PostgresIncomingWebhookStore {
    async fn create_webhook(&self, webhook: IncomingWebhook) -> Result<(), WebhookError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO incoming_webhooks (
                    id, organization_id, space_id, channel_id, bot_user_id, created_by_user_id,
                    name, token_hash, token_last_four, status, created_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6::uuid,
                    $7, $8, $9, $10, $11::timestamptz
                )
                "#,
                vec![
                    Value::from(webhook.id.to_string()),
                    Value::from(webhook.organization_id.to_string()),
                    Value::from(webhook.space_id.to_string()),
                    Value::from(webhook.channel_id.to_string()),
                    Value::from(webhook.bot_user_id.to_string()),
                    Value::from(webhook.created_by_user_id.to_string()),
                    Value::from(webhook.name),
                    Value::from(webhook.token_hash),
                    Value::from(webhook.token_last_four),
                    Value::from(webhook.status),
                    Value::from(webhook.created_at),
                ],
            ))
            .await
            .map_err(|_| WebhookError::StoreUnavailable)?;

        Ok(())
    }

    async fn get_webhook(&self, webhook_id: Uuid) -> Result<Option<IncomingWebhook>, WebhookError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, channel_id::text,
                       bot_user_id::text, created_by_user_id::text, name, token_hash,
                       token_last_four, status, created_at::text
                FROM incoming_webhooks
                WHERE id = $1::uuid
                "#,
                vec![Value::from(webhook_id.to_string())],
            ))
            .await
            .map_err(|_| WebhookError::StoreUnavailable)?;

        row.map(webhook_from_row).transpose()
    }

    async fn list_webhooks_for_channel(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<IncomingWebhook>, WebhookError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, channel_id::text,
                       bot_user_id::text, created_by_user_id::text, name, token_hash,
                       token_last_four, status, created_at::text
                FROM incoming_webhooks
                WHERE channel_id = $1::uuid
                  AND status = 'active'
                ORDER BY created_at ASC, id ASC
                "#,
                vec![Value::from(channel_id.to_string())],
            ))
            .await
            .map_err(|_| WebhookError::StoreUnavailable)?;

        rows.into_iter()
            .map(webhook_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn rotate_webhook_token(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
        token_hash: String,
        token_last_four: String,
    ) -> Result<Option<IncomingWebhook>, WebhookError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE incoming_webhooks
                SET token_hash = $3,
                    token_last_four = $4,
                    updated_at = now()
                WHERE id = $1::uuid
                  AND channel_id = $2::uuid
                  AND status = 'active'
                RETURNING id::text, organization_id::text, space_id::text, channel_id::text,
                          bot_user_id::text, created_by_user_id::text, name, token_hash,
                          token_last_four, status, created_at::text
                "#,
                vec![
                    Value::from(webhook_id.to_string()),
                    Value::from(channel_id.to_string()),
                    Value::from(token_hash),
                    Value::from(token_last_four),
                ],
            ))
            .await
            .map_err(|_| WebhookError::StoreUnavailable)?;

        row.map(webhook_from_row).transpose()
    }

    async fn disable_webhook(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
    ) -> Result<bool, WebhookError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE incoming_webhooks
                SET status = 'disabled',
                    updated_at = now()
                WHERE id = $1::uuid
                  AND channel_id = $2::uuid
                  AND status = 'active'
                "#,
                vec![
                    Value::from(webhook_id.to_string()),
                    Value::from(channel_id.to_string()),
                ],
            ))
            .await
            .map_err(|_| WebhookError::StoreUnavailable)?;

        Ok(result.rows_affected() > 0)
    }
}

fn webhook_from_row(row: sea_orm::QueryResult) -> Result<IncomingWebhook, WebhookError> {
    Ok(IncomingWebhook {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        space_id: parse_uuid(
            &row.try_get::<String>("", "space_id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        channel_id: parse_uuid(
            &row.try_get::<String>("", "channel_id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        bot_user_id: parse_uuid(
            &row.try_get::<String>("", "bot_user_id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        created_by_user_id: parse_uuid(
            &row.try_get::<String>("", "created_by_user_id")
                .map_err(|_| WebhookError::StoreUnavailable)?,
        )?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| WebhookError::StoreUnavailable)?,
        token_hash: row
            .try_get::<String>("", "token_hash")
            .map_err(|_| WebhookError::StoreUnavailable)?,
        token_last_four: row
            .try_get::<String>("", "token_last_four")
            .map_err(|_| WebhookError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| WebhookError::StoreUnavailable)?,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| WebhookError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, WebhookError> {
    Uuid::parse_str(value).map_err(|_| WebhookError::StoreUnavailable)
}
