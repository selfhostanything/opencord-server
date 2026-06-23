use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::push::{PushError, PushPlatform, PushToken, PushTokenStore};

#[derive(Clone)]
pub struct PostgresPushTokenStore {
    db: DatabaseConnection,
}

impl PostgresPushTokenStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl PushTokenStore for PostgresPushTokenStore {
    async fn upsert_token(&self, token: PushToken) -> Result<PushToken, PushError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO push_tokens (
                    id, user_id, platform, token, token_hash, token_last_four,
                    device_name, created_at, updated_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3, $4, $5, $6,
                    $7, $8::timestamptz, $9::timestamptz
                )
                ON CONFLICT (user_id, platform, token_hash)
                DO UPDATE SET
                    token = EXCLUDED.token,
                    token_last_four = EXCLUDED.token_last_four,
                    device_name = EXCLUDED.device_name,
                    updated_at = EXCLUDED.updated_at
                RETURNING id::text, user_id::text, platform, token, token_hash,
                          token_last_four, device_name, created_at::text, updated_at::text
                "#,
                push_token_values(&token),
            ))
            .await
            .map_err(|_| PushError::StoreUnavailable)?;

        row.map(push_token_from_row)
            .transpose()?
            .ok_or(PushError::StoreUnavailable)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<PushToken>, PushError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, user_id::text, platform, token, token_hash,
                       token_last_four, device_name, created_at::text, updated_at::text
                FROM push_tokens
                WHERE user_id = $1::uuid
                ORDER BY created_at ASC, id ASC
                "#,
                vec![Value::from(user_id.to_string())],
            ))
            .await
            .map_err(|_| PushError::StoreUnavailable)?;

        rows.into_iter()
            .map(push_token_from_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn push_token_from_row(row: sea_orm::QueryResult) -> Result<PushToken, PushError> {
    let platform = row
        .try_get::<String>("", "platform")
        .map_err(|_| PushError::StoreUnavailable)?;

    Ok(PushToken {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| PushError::StoreUnavailable)?,
        )?,
        user_id: parse_uuid(
            &row.try_get::<String>("", "user_id")
                .map_err(|_| PushError::StoreUnavailable)?,
        )?,
        platform: PushPlatform::parse(&platform).map_err(|_| PushError::StoreUnavailable)?,
        token: row
            .try_get::<String>("", "token")
            .map_err(|_| PushError::StoreUnavailable)?,
        token_hash: row
            .try_get::<String>("", "token_hash")
            .map_err(|_| PushError::StoreUnavailable)?,
        token_last_four: row
            .try_get::<String>("", "token_last_four")
            .map_err(|_| PushError::StoreUnavailable)?,
        device_name: row
            .try_get::<Option<String>>("", "device_name")
            .map_err(|_| PushError::StoreUnavailable)?,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| PushError::StoreUnavailable)?,
        updated_at: row
            .try_get::<String>("", "updated_at")
            .map_err(|_| PushError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, PushError> {
    Uuid::parse_str(value).map_err(|_| PushError::StoreUnavailable)
}

fn push_token_values(token: &PushToken) -> Vec<Value> {
    vec![
        Value::from(token.id.to_string()),
        Value::from(token.user_id.to_string()),
        Value::from(token.platform.as_str()),
        Value::from(token.token.clone()),
        Value::from(token.token_hash.clone()),
        Value::from(token.token_last_four.clone()),
        Value::from(token.device_name.clone()),
        Value::from(token.created_at.clone()),
        Value::from(token.updated_at.clone()),
    ]
}
