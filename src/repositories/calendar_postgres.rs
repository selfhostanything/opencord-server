use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::calendar_sync::{
    CalendarEventSync, CalendarStore, CalendarSyncError, ConnectedCalendarAccount,
};

#[derive(Clone)]
pub struct PostgresCalendarStore {
    db: DatabaseConnection,
}

impl PostgresCalendarStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl CalendarStore for PostgresCalendarStore {
    async fn upsert_account(
        &self,
        account: ConnectedCalendarAccount,
    ) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO connected_calendar_accounts (
                    id, user_id, provider, external_account_id, calendar_id,
                    access_token_ciphertext, refresh_token_ciphertext, token_last_four,
                    sync_enabled, created_at, updated_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3, $4, $5,
                    $6, $7, $8,
                    $9, $10::timestamptz, $11::timestamptz
                )
                ON CONFLICT (user_id, provider)
                DO UPDATE SET
                    external_account_id = EXCLUDED.external_account_id,
                    calendar_id = EXCLUDED.calendar_id,
                    access_token_ciphertext = EXCLUDED.access_token_ciphertext,
                    refresh_token_ciphertext = EXCLUDED.refresh_token_ciphertext,
                    token_last_four = EXCLUDED.token_last_four,
                    sync_enabled = EXCLUDED.sync_enabled,
                    updated_at = EXCLUDED.updated_at
                RETURNING id::text, user_id::text, provider, external_account_id, calendar_id,
                          access_token_ciphertext, refresh_token_ciphertext, token_last_four,
                          sync_enabled,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                "#,
                account_values(&account),
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        row.map(account_from_row)
            .transpose()?
            .ok_or(CalendarSyncError::StoreUnavailable)
    }

    async fn list_accounts(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ConnectedCalendarAccount>, CalendarSyncError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, user_id::text, provider, external_account_id, calendar_id,
                       access_token_ciphertext, refresh_token_ciphertext, token_last_four,
                       sync_enabled,
                       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                FROM connected_calendar_accounts
                WHERE user_id = $1::uuid
                ORDER BY provider ASC
                "#,
                vec![Value::from(user_id.to_string())],
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        rows.into_iter()
            .map(account_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn connected_account(
        &self,
        user_id: Uuid,
        provider: String,
    ) -> Result<Option<ConnectedCalendarAccount>, CalendarSyncError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, user_id::text, provider, external_account_id, calendar_id,
                       access_token_ciphertext, refresh_token_ciphertext, token_last_four,
                       sync_enabled,
                       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                FROM connected_calendar_accounts
                WHERE user_id = $1::uuid
                  AND provider = $2
                "#,
                vec![Value::from(user_id.to_string()), Value::from(provider)],
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        row.map(account_from_row).transpose()
    }

    async fn event_sync_for_meeting(
        &self,
        meeting_id: Uuid,
        account_id: Uuid,
        provider: String,
    ) -> Result<Option<CalendarEventSync>, CalendarSyncError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, meeting_id::text, account_id::text, provider,
                       provider_event_id, provider_event_url, calendar_id, status,
                       to_char(last_synced_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_synced_at,
                       failure_reason,
                       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                FROM calendar_event_syncs
                WHERE meeting_id = $1::uuid
                  AND account_id = $2::uuid
                  AND provider = $3
                "#,
                vec![
                    Value::from(meeting_id.to_string()),
                    Value::from(account_id.to_string()),
                    Value::from(provider),
                ],
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        row.map(event_sync_from_row).transpose()
    }

    async fn upsert_event_sync(
        &self,
        event: CalendarEventSync,
    ) -> Result<CalendarEventSync, CalendarSyncError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO calendar_event_syncs (
                    id, meeting_id, account_id, provider, provider_event_id,
                    provider_event_url, calendar_id, status, last_synced_at,
                    failure_reason, created_at, updated_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4, $5,
                    $6, $7, $8, $9::timestamptz,
                    $10, $11::timestamptz, $12::timestamptz
                )
                ON CONFLICT (meeting_id, account_id, provider)
                DO UPDATE SET
                    provider_event_id = EXCLUDED.provider_event_id,
                    provider_event_url = EXCLUDED.provider_event_url,
                    calendar_id = EXCLUDED.calendar_id,
                    status = EXCLUDED.status,
                    last_synced_at = EXCLUDED.last_synced_at,
                    failure_reason = EXCLUDED.failure_reason,
                    updated_at = EXCLUDED.updated_at
                RETURNING id::text, meeting_id::text, account_id::text, provider,
                          provider_event_id, provider_event_url, calendar_id, status,
                          to_char(last_synced_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_synced_at,
                          failure_reason,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                "#,
                event_sync_values(&event),
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        row.map(event_sync_from_row)
            .transpose()?
            .ok_or(CalendarSyncError::StoreUnavailable)
    }

    async fn count_accounts_for_user_ids(
        &self,
        user_ids: &[Uuid],
    ) -> Result<i64, CalendarSyncError> {
        if user_ids.is_empty() {
            return Ok(0);
        }

        let placeholders = (1..=user_ids.len())
            .map(|index| format!("${index}::uuid"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = user_ids
            .iter()
            .map(|user_id| Value::from(user_id.to_string()))
            .collect::<Vec<_>>();
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                format!(
                    r#"
                    SELECT COUNT(*)::bigint AS connected_accounts
                    FROM connected_calendar_accounts
                    WHERE user_id IN ({placeholders})
                      AND sync_enabled = true
                    "#
                ),
                values,
            ))
            .await
            .map_err(|_| CalendarSyncError::StoreUnavailable)?;

        row.ok_or(CalendarSyncError::StoreUnavailable)?
            .try_get::<i64>("", "connected_accounts")
            .map_err(|_| CalendarSyncError::StoreUnavailable)
    }
}

fn account_values(account: &ConnectedCalendarAccount) -> Vec<Value> {
    vec![
        Value::from(account.id.to_string()),
        Value::from(account.user_id.to_string()),
        Value::from(account.provider.clone()),
        Value::from(account.external_account_id.clone()),
        Value::from(account.calendar_id.clone()),
        Value::from(account.access_token_ciphertext.clone()),
        Value::from(account.refresh_token_ciphertext.clone()),
        Value::from(account.token_last_four.clone()),
        Value::from(account.sync_enabled),
        Value::from(account.created_at.clone()),
        Value::from(account.updated_at.clone()),
    ]
}

fn event_sync_values(event: &CalendarEventSync) -> Vec<Value> {
    vec![
        Value::from(event.id.to_string()),
        Value::from(event.meeting_id.to_string()),
        Value::from(event.account_id.to_string()),
        Value::from(event.provider.clone()),
        Value::from(event.provider_event_id.clone()),
        Value::from(event.provider_event_url.clone()),
        Value::from(event.calendar_id.clone()),
        Value::from(event.status.clone()),
        Value::from(event.last_synced_at.clone()),
        Value::from(event.failure_reason.clone()),
        Value::from(event.created_at.clone()),
        Value::from(event.updated_at.clone()),
    ]
}

fn account_from_row(
    row: sea_orm::QueryResult,
) -> Result<ConnectedCalendarAccount, CalendarSyncError> {
    Ok(ConnectedCalendarAccount {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        )?,
        user_id: parse_uuid(
            &row.try_get::<String>("", "user_id")
                .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        )?,
        provider: row
            .try_get::<String>("", "provider")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        external_account_id: row
            .try_get::<String>("", "external_account_id")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        calendar_id: row
            .try_get::<String>("", "calendar_id")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        access_token_ciphertext: row
            .try_get::<String>("", "access_token_ciphertext")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        refresh_token_ciphertext: row
            .try_get::<Option<String>>("", "refresh_token_ciphertext")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        token_last_four: row
            .try_get::<String>("", "token_last_four")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        sync_enabled: row
            .try_get::<bool>("", "sync_enabled")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        updated_at: row
            .try_get::<String>("", "updated_at")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
    })
}

fn event_sync_from_row(row: sea_orm::QueryResult) -> Result<CalendarEventSync, CalendarSyncError> {
    Ok(CalendarEventSync {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        )?,
        meeting_id: parse_uuid(
            &row.try_get::<String>("", "meeting_id")
                .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        )?,
        account_id: parse_uuid(
            &row.try_get::<String>("", "account_id")
                .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        )?,
        provider: row
            .try_get::<String>("", "provider")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        provider_event_id: row
            .try_get::<String>("", "provider_event_id")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        provider_event_url: row
            .try_get::<Option<String>>("", "provider_event_url")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        calendar_id: row
            .try_get::<String>("", "calendar_id")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        last_synced_at: row
            .try_get::<String>("", "last_synced_at")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        failure_reason: row
            .try_get::<Option<String>>("", "failure_reason")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        created_at: row
            .try_get::<String>("", "created_at")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
        updated_at: row
            .try_get::<String>("", "updated_at")
            .map_err(|_| CalendarSyncError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, CalendarSyncError> {
    Uuid::parse_str(value).map_err(|_| CalendarSyncError::StoreUnavailable)
}
