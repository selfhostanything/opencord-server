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
                    content, content_format
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                    $6, $7
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
                SET content = $6,
                    edited_at = now()
                WHERE id = $1::uuid
                  AND deleted_at IS NULL
                RETURNING id::text, organization_id::text, space_id::text, channel_id::text,
                          author_user_id::text, content, content_format,
                          edited_at::text, deleted_at::text
                "#,
                message_values(&message),
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
                message_values(&message),
            ))
            .await
            .map_err(|_| MessageError::StoreUnavailable)?;

        if result.rows_affected() == 0 {
            Err(MessageError::NotFound)
        } else {
            Ok(())
        }
    }
}

fn message_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, organization_id::text, space_id::text, channel_id::text,
               author_user_id::text, content, content_format,
               edited_at::text, deleted_at::text
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
        edited_at: row
            .try_get::<Option<String>>("", "edited_at")
            .map_err(|_| MessageError::StoreUnavailable)?,
        deleted_at: row
            .try_get::<Option<String>>("", "deleted_at")
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
    ]
}
