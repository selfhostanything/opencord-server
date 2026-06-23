use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::audit::{AuditError, AuditEvent, AuditStore};

#[derive(Clone)]
pub struct PostgresAuditStore {
    db: DatabaseConnection,
}

impl PostgresAuditStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AuditStore for PostgresAuditStore {
    async fn create_event(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO audit_events (
                    id, organization_id, space_id, actor_user_id,
                    action, target_type, target_id, metadata, created_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid,
                    $5, $6, $7::uuid, $8::jsonb, $9::timestamptz
                )
                "#,
                event_values(&event),
            ))
            .await
            .map_err(|_| AuditError::StoreUnavailable)?;

        Ok(())
    }

    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, actor_user_id::text,
                       action, target_type, target_id::text, metadata::text, created_at::text
                FROM audit_events
                WHERE space_id = $1::uuid
                ORDER BY created_at ASC, id ASC
                "#,
                vec![Value::from(space_id.to_string())],
            ))
            .await
            .map_err(|_| AuditError::StoreUnavailable)?;

        rows.into_iter()
            .map(event_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, space_id::text, actor_user_id::text,
                       action, target_type, target_id::text, metadata::text, created_at::text
                FROM audit_events
                WHERE organization_id = $1::uuid
                  AND created_at >= $2::timestamptz
                  AND created_at <= $3::timestamptz
                ORDER BY created_at ASC, id ASC
                "#,
                vec![
                    Value::from(organization_id.to_string()),
                    Value::from(from),
                    Value::from(to),
                ],
            ))
            .await
            .map_err(|_| AuditError::StoreUnavailable)?;

        rows.into_iter()
            .map(event_from_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn event_from_row(row: sea_orm::QueryResult) -> Result<AuditEvent, AuditError> {
    let metadata = row
        .try_get::<String>("", "metadata")
        .map_err(|_| AuditError::StoreUnavailable)?;
    let metadata = serde_json::from_str(&metadata).map_err(|_| AuditError::StoreUnavailable)?;

    Ok(AuditEvent {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| AuditError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| AuditError::StoreUnavailable)?,
        )?,
        space_id: parse_uuid(
            &row.try_get::<String>("", "space_id")
                .map_err(|_| AuditError::StoreUnavailable)?,
        )?,
        actor_user_id: parse_uuid(
            &row.try_get::<String>("", "actor_user_id")
                .map_err(|_| AuditError::StoreUnavailable)?,
        )?,
        action: row
            .try_get::<String>("", "action")
            .map_err(|_| AuditError::StoreUnavailable)?,
        target_type: row
            .try_get::<String>("", "target_type")
            .map_err(|_| AuditError::StoreUnavailable)?,
        target_id: parse_uuid(
            &row.try_get::<String>("", "target_id")
                .map_err(|_| AuditError::StoreUnavailable)?,
        )?,
        metadata,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| AuditError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, AuditError> {
    Uuid::parse_str(value).map_err(|_| AuditError::StoreUnavailable)
}

fn event_values(event: &AuditEvent) -> Vec<Value> {
    vec![
        Value::from(event.id.to_string()),
        Value::from(event.organization_id.to_string()),
        Value::from(event.space_id.to_string()),
        Value::from(event.actor_user_id.to_string()),
        Value::from(event.action.clone()),
        Value::from(event.target_type.clone()),
        Value::from(event.target_id.to_string()),
        Value::from(event.metadata.to_string()),
        Value::from(event.created_at.clone()),
    ]
}
