use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::message::{Message, MessageError, MessageStore};

#[derive(Clone)]
pub struct PostgresMessageStore {
    db: DatabaseConnection,
}

impl PostgresMessageStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl MessageStore for PostgresMessageStore {
    async fn create_message(&self, message: Message) -> Result<(), MessageError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO messages (
                    id, organization_id, space_id, channel_id, author_user_id,
                    content, content_format, embeds, reply_to_message_id, created_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                    $6, $7, $8::jsonb, $9::uuid, $10::timestamptz
                )
                "#,
                message_values(&message),
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        Ok(())
    }

    async fn list_for_channel(&self, channel_id: Uuid) -> Result<Vec<Message>, MessageError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                message_select_sql(
                    r#"
                    WHERE channel_id = $1::uuid
                      AND deleted_at IS NULL
                    ORDER BY created_at ASC, id ASC
                    "#,
                ),
                vec![Value::from(channel_id.to_string())],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        rows.into_iter()
            .map(message_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<Message>, MessageError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                message_select_sql(
                    r#"
                    WHERE organization_id = $1::uuid
                      AND created_at >= $2::timestamptz
                      AND created_at <= $3::timestamptz
                    ORDER BY created_at ASC, id ASC
                    "#,
                ),
                vec![
                    Value::from(organization_id.to_string()),
                    Value::from(from),
                    Value::from(to),
                ],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        rows.into_iter()
            .map(message_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_message(&self, message_id: Uuid) -> Result<Option<Message>, MessageError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                message_select_sql(
                    r#"
                    WHERE id = $1::uuid
                      AND deleted_at IS NULL
                    "#,
                ),
                vec![Value::from(message_id.to_string())],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        row.map(message_from_row).transpose()
    }

    async fn update_message(&self, message: Message) -> Result<Message, MessageError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE messages
                SET content = $2,
                    edited_at = now()
                WHERE id = $1::uuid
                  AND deleted_at IS NULL
                RETURNING id::text, organization_id::text, space_id::text, channel_id::text,
                          author_user_id::text, content, content_format, embeds::text,
                          reply_to_message_id::text, edited_at::text, deleted_at::text,
                          created_at::text
                "#,
                vec![
                    Value::from(message.id.to_string()),
                    Value::from(message.content.clone()),
                ],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        row.map(message_from_row)
            .transpose()?
            .ok_or(MessageError::NotFound)
    }

    async fn delete_message(&self, message: Message) -> Result<(), MessageError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE messages
                SET deleted_at = now()
                WHERE id = $1::uuid
                  AND deleted_at IS NULL
                "#,
                vec![Value::from(message.id.to_string())],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        if result.rows_affected() == 0 {
            Err(MessageError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn purge_for_retention(
        &self,
        organization_id: Uuid,
        created_before: Option<String>,
        deleted_before: Option<String>,
        dry_run: bool,
    ) -> Result<usize, MessageError> {
        if created_before.is_none() && deleted_before.is_none() {
            return Ok(0);
        }

        let sql = if dry_run {
            r#"
            SELECT COUNT(*)::bigint AS purged_count
            FROM messages
            WHERE organization_id = $1::uuid
              AND (
                  ($2::timestamptz IS NOT NULL AND created_at < $2::timestamptz)
                  OR ($3::timestamptz IS NOT NULL AND deleted_at IS NOT NULL AND deleted_at < $3::timestamptz)
              )
            "#
        } else {
            r#"
            WITH deleted AS (
                DELETE FROM messages
                WHERE organization_id = $1::uuid
                  AND (
                      ($2::timestamptz IS NOT NULL AND created_at < $2::timestamptz)
                      OR ($3::timestamptz IS NOT NULL AND deleted_at IS NOT NULL AND deleted_at < $3::timestamptz)
                  )
                RETURNING id
            )
            SELECT COUNT(*)::bigint AS purged_count
            FROM deleted
            "#
        };

        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                vec![
                    Value::from(organization_id.to_string()),
                    Value::from(created_before),
                    Value::from(deleted_before),
                ],
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        let count = row
            .ok_or(MessageError::StoreUnavailable)?
            .try_get::<i64>("", "purged_count")
            .map_err(|_| MessageError::StoreUnavailable)?;
        Ok(count as usize)
    }
}

fn message_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, organization_id::text, space_id::text, channel_id::text,
               author_user_id::text, content, content_format, embeds::text,
               reply_to_message_id::text, edited_at::text, deleted_at::text, created_at::text
        FROM messages
        {where_clause}
        "#
    )
}

fn message_from_row(row: sea_orm::QueryResult) -> Result<Message, MessageError> {
    let space_id = row
        .try_get::<Option<String>>("", "space_id")
        .map_err(|_| MessageError::StoreUnavailable)?
        .map(|id| Uuid::parse_str(&id).map_err(|_| MessageError::StoreUnavailable))
        .transpose()?;
    let reply_to_message_id = row
        .try_get::<Option<String>>("", "reply_to_message_id")
        .map_err(|_| MessageError::StoreUnavailable)?
        .map(|id| Uuid::parse_str(&id).map_err(|_| MessageError::StoreUnavailable))
        .transpose()?;

    Ok(Message {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| MessageError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| MessageError::StoreUnavailable)?,
        )?,
        space_id,
        channel_id: parse_uuid(
            &row.try_get::<String>("", "channel_id")
                .map_err(|_| MessageError::StoreUnavailable)?,
        )?,
        author_user_id: parse_uuid(
            &row.try_get::<String>("", "author_user_id")
                .map_err(|_| MessageError::StoreUnavailable)?,
        )?,
        content: row
            .try_get::<String>("", "content")
            .map_err(|_| MessageError::StoreUnavailable)?,
        content_format: row
            .try_get::<String>("", "content_format")
            .map_err(|_| MessageError::StoreUnavailable)?,
        embeds: parse_embeds(
            &row.try_get::<String>("", "embeds")
                .map_err(|_| MessageError::StoreUnavailable)?,
        )?,
        reply_to_message_id,
        edited_at: row
            .try_get::<Option<String>>("", "edited_at")
            .map_err(|_| MessageError::StoreUnavailable)?,
        deleted_at: row
            .try_get::<Option<String>>("", "deleted_at")
            .map_err(|_| MessageError::StoreUnavailable)?,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| MessageError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, MessageError> {
    Uuid::parse_str(value).map_err(|_| MessageError::StoreUnavailable)
}

fn message_values(message: &Message) -> Vec<Value> {
    vec![
        Value::from(message.id.to_string()),
        Value::from(message.organization_id.to_string()),
        Value::from(message.space_id.map(|id| id.to_string())),
        Value::from(message.channel_id.to_string()),
        Value::from(message.author_user_id.to_string()),
        Value::from(message.content.clone()),
        Value::from(message.content_format.clone()),
        Value::from(serde_json::Value::Array(message.embeds.clone()).to_string()),
        Value::from(message.reply_to_message_id.map(|id| id.to_string())),
        Value::from(message.created_at.clone()),
    ]
}

fn parse_embeds(value: &str) -> Result<Vec<serde_json::Value>, MessageError> {
    serde_json::from_str(value).map_err(|_| MessageError::StoreUnavailable)
}
