use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, Value};
use uuid::Uuid;

use crate::domain::channel::{Channel, ChannelError, ChannelStore};

#[derive(Clone)]
pub struct PostgresChannelStore {
    db: DatabaseConnection,
}

impl PostgresChannelStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ChannelStore for PostgresChannelStore {
    async fn create_channel(&self, channel: Channel) -> Result<(), ChannelError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO channels (
                    id, organization_id, space_id, kind, name, slug,
                    position, topic, is_private
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4, $5, $6,
                    $7, NULLIF($8, ''), $9
                )
                "#,
                channel_values(&channel),
            ))
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(ChannelError::SlugAlreadyExists),
            Err(_) => Err(ChannelError::StoreUnavailable),
        }
    }

    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<Channel>, ChannelError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, kind,
                       name, slug, position, topic, is_private
                FROM channels
                WHERE space_id = $1::uuid
                  AND archived_at IS NULL
                ORDER BY position ASC, name ASC
                "#,
                vec![Value::from(space_id.to_string())],
            ))
            .await
            .map_err(|_| ChannelError::StoreUnavailable)?;

        rows.into_iter()
            .map(channel_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_channel(&self, channel_id: Uuid) -> Result<Option<Channel>, ChannelError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, kind,
                       name, slug, position, topic, is_private
                FROM channels
                WHERE id = $1::uuid
                  AND archived_at IS NULL
                "#,
                vec![Value::from(channel_id.to_string())],
            ))
            .await
            .map_err(|_| ChannelError::StoreUnavailable)?;

        row.map(channel_from_row).transpose()
    }

    async fn update_channel(&self, channel: Channel) -> Result<Channel, ChannelError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE channels
                SET name = $5,
                    slug = $6,
                    position = $7,
                    topic = NULLIF($8, ''),
                    is_private = $9
                WHERE id = $1::uuid
                  AND archived_at IS NULL
                "#,
                channel_values(&channel),
            ))
            .await;

        match result {
            Ok(result) if result.rows_affected() == 0 => Err(ChannelError::NotFound),
            Ok(_) => Ok(channel),
            Err(error) if is_unique_violation(&error) => Err(ChannelError::SlugAlreadyExists),
            Err(_) => Err(ChannelError::StoreUnavailable),
        }
    }
}

fn channel_from_row(row: sea_orm::QueryResult) -> Result<Channel, ChannelError> {
    Ok(Channel {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| ChannelError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| ChannelError::StoreUnavailable)?,
        )?,
        space_id: parse_uuid(
            &row.try_get::<String>("", "space_id")
                .map_err(|_| ChannelError::StoreUnavailable)?,
        )?,
        kind: row
            .try_get::<String>("", "kind")
            .map_err(|_| ChannelError::StoreUnavailable)?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| ChannelError::StoreUnavailable)?,
        slug: row
            .try_get::<String>("", "slug")
            .map_err(|_| ChannelError::StoreUnavailable)?,
        position: row
            .try_get::<i32>("", "position")
            .map_err(|_| ChannelError::StoreUnavailable)?,
        topic: row
            .try_get::<Option<String>>("", "topic")
            .map_err(|_| ChannelError::StoreUnavailable)?,
        is_private: row
            .try_get::<bool>("", "is_private")
            .map_err(|_| ChannelError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, ChannelError> {
    Uuid::parse_str(value).map_err(|_| ChannelError::StoreUnavailable)
}

fn channel_values(channel: &Channel) -> Vec<Value> {
    vec![
        Value::from(channel.id.to_string()),
        Value::from(channel.organization_id.to_string()),
        Value::from(channel.space_id.to_string()),
        Value::from(channel.kind.clone()),
        Value::from(channel.name.clone()),
        Value::from(channel.slug.clone()),
        Value::from(channel.position),
        Value::from(channel.topic.clone().unwrap_or_default()),
        Value::from(channel.is_private),
    ]
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}
