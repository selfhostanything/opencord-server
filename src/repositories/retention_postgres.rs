use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::retention::{RetentionError, RetentionPolicy, RetentionRun, RetentionStore};

#[derive(Clone)]
pub struct PostgresRetentionStore {
    db: DatabaseConnection,
}

impl PostgresRetentionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl RetentionStore for PostgresRetentionStore {
    async fn upsert_policy(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organization_retention_policies (
                    organization_id, messages_retain_days, files_retain_days,
                    audit_logs_retain_days, deleted_message_purge_days
                )
                VALUES ($1::uuid, $2, $3, $4, $5)
                ON CONFLICT (organization_id)
                DO UPDATE SET
                    messages_retain_days = EXCLUDED.messages_retain_days,
                    files_retain_days = EXCLUDED.files_retain_days,
                    audit_logs_retain_days = EXCLUDED.audit_logs_retain_days,
                    deleted_message_purge_days = EXCLUDED.deleted_message_purge_days,
                    updated_at = now()
                RETURNING organization_id::text, messages_retain_days, files_retain_days,
                          audit_logs_retain_days, deleted_message_purge_days
                "#,
                policy_values(&policy),
            ))
            .await
            .map_err(|_| RetentionError::StoreUnavailable)?;

        row.map(policy_from_row)
            .transpose()?
            .ok_or(RetentionError::StoreUnavailable)
    }

    async fn get_policy(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                retention_policy_select_sql(
                    r#"
                    WHERE organization_id = $1::uuid
                    "#,
                ),
                vec![Value::from(organization_id.to_string())],
            ))
            .await
            .map_err(|_| RetentionError::StoreUnavailable)?;

        row.map(policy_from_row).transpose()
    }

    async fn list_policies(&self) -> Result<Vec<RetentionPolicy>, RetentionError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                retention_policy_select_sql(
                    r#"
                    ORDER BY organization_id ASC
                    "#,
                ),
                vec![],
            ))
            .await
            .map_err(|_| RetentionError::StoreUnavailable)?;

        rows.into_iter()
            .map(policy_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn record_run(&self, run: RetentionRun) -> Result<(), RetentionError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO retention_runs (
                    id, organization_id, dry_run, messages_purged, files_purged,
                    audit_events_purged, ran_at
                )
                VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7::timestamptz)
                "#,
                vec![
                    Value::from(run.id.to_string()),
                    Value::from(run.organization_id.to_string()),
                    Value::from(run.dry_run),
                    Value::from(run.messages_purged as i64),
                    Value::from(run.files_purged as i64),
                    Value::from(run.audit_events_purged as i64),
                    Value::from(run.ran_at),
                ],
            ))
            .await
            .map_err(|_| RetentionError::StoreUnavailable)?;

        Ok(())
    }

    async fn list_runs_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<RetentionRun>, RetentionError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, dry_run, messages_purged,
                       files_purged, audit_events_purged, ran_at::text
                FROM retention_runs
                WHERE organization_id = $1::uuid
                ORDER BY ran_at ASC, id ASC
                "#,
                vec![Value::from(organization_id.to_string())],
            ))
            .await
            .map_err(|_| RetentionError::StoreUnavailable)?;

        rows.into_iter()
            .map(run_from_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn retention_policy_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT organization_id::text, messages_retain_days, files_retain_days,
               audit_logs_retain_days, deleted_message_purge_days
        FROM organization_retention_policies
        {where_clause}
        "#
    )
}

fn policy_from_row(row: sea_orm::QueryResult) -> Result<RetentionPolicy, RetentionError> {
    Ok(RetentionPolicy {
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| RetentionError::StoreUnavailable)?,
        )?,
        messages_retain_days: row
            .try_get::<Option<i32>>("", "messages_retain_days")
            .map_err(|_| RetentionError::StoreUnavailable)?
            .map(i64::from),
        files_retain_days: row
            .try_get::<Option<i32>>("", "files_retain_days")
            .map_err(|_| RetentionError::StoreUnavailable)?
            .map(i64::from),
        audit_logs_retain_days: row
            .try_get::<Option<i32>>("", "audit_logs_retain_days")
            .map_err(|_| RetentionError::StoreUnavailable)?
            .map(i64::from),
        deleted_message_purge_days: row
            .try_get::<Option<i32>>("", "deleted_message_purge_days")
            .map_err(|_| RetentionError::StoreUnavailable)?
            .map(i64::from),
    })
}

fn run_from_row(row: sea_orm::QueryResult) -> Result<RetentionRun, RetentionError> {
    Ok(RetentionRun {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| RetentionError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| RetentionError::StoreUnavailable)?,
        )?,
        dry_run: row
            .try_get::<bool>("", "dry_run")
            .map_err(|_| RetentionError::StoreUnavailable)?,
        messages_purged: row
            .try_get::<i64>("", "messages_purged")
            .map_err(|_| RetentionError::StoreUnavailable)? as usize,
        files_purged: row
            .try_get::<i64>("", "files_purged")
            .map_err(|_| RetentionError::StoreUnavailable)? as usize,
        audit_events_purged: row
            .try_get::<i64>("", "audit_events_purged")
            .map_err(|_| RetentionError::StoreUnavailable)? as usize,
        ran_at: row
            .try_get::<String>("", "ran_at")
            .map_err(|_| RetentionError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, RetentionError> {
    Uuid::parse_str(value).map_err(|_| RetentionError::StoreUnavailable)
}

fn policy_values(policy: &RetentionPolicy) -> Vec<Value> {
    vec![
        Value::from(policy.organization_id.to_string()),
        Value::from(policy.messages_retain_days.map(|days| days as i32)),
        Value::from(policy.files_retain_days.map(|days| days as i32)),
        Value::from(policy.audit_logs_retain_days.map(|days| days as i32)),
        Value::from(policy.deleted_message_purge_days.map(|days| days as i32)),
    ]
}
