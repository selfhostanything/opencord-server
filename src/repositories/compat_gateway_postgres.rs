use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};

use uuid::Uuid;

use crate::domain::bot::AuthenticatedBot;
use crate::domain::compat_gateway::{
    CompatGatewayError, CompatGatewayReplayEvent, CompatGatewayResumeResult, CompatGatewaySession,
    CompatGatewaySessionStore,
};

#[derive(Clone)]
pub struct PostgresCompatGatewaySessionStore {
    db: DatabaseConnection,
}

impl PostgresCompatGatewaySessionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl CompatGatewaySessionStore for PostgresCompatGatewaySessionStore {
    async fn create_session(
        &self,
        session: CompatGatewaySession,
    ) -> Result<CompatGatewaySession, CompatGatewayError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO compat_gateway_sessions (
                    session_id, application_id, organization_id, bot_user_id, sequence, intents
                )
                VALUES ($1, $2::uuid, $3::uuid, $4::uuid, $5, $6)
                ON CONFLICT (session_id) DO UPDATE
                SET application_id = EXCLUDED.application_id,
                    organization_id = EXCLUDED.organization_id,
                    bot_user_id = EXCLUDED.bot_user_id,
                    sequence = EXCLUDED.sequence,
                    intents = EXCLUDED.intents,
                    expires_at = now() + interval '24 hours',
                    updated_at = now()
                RETURNING session_id, application_id::text, organization_id::text,
                          bot_user_id::text, sequence, intents
                "#,
                session_values(&session),
            ))
            .await
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        row.map(session_from_row)
            .transpose()?
            .ok_or(CompatGatewayError::StoreUnavailable)
    }

    async fn update_sequence(
        &self,
        session_id: &str,
        sequence: i64,
    ) -> Result<(), CompatGatewayError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE compat_gateway_sessions
                SET sequence = GREATEST(sequence, $2),
                    updated_at = now()
                WHERE session_id = $1
                  AND expires_at > now()
                "#,
                vec![Value::from(session_id.to_owned()), Value::from(sequence)],
            ))
            .await
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        Ok(())
    }

    async fn resume_session(
        &self,
        session_id: &str,
        bot: &AuthenticatedBot,
        client_sequence: i64,
    ) -> Result<CompatGatewayResumeResult, CompatGatewayError> {
        let Some(mut session) = self.get_active_session(session_id).await? else {
            return Ok(CompatGatewayResumeResult::NotFound);
        };
        if session.application_id != bot.application_id
            || session.organization_id != bot.organization_id
            || session.bot_user_id != bot.bot_user_id
        {
            return Ok(CompatGatewayResumeResult::NotFound);
        }
        if client_sequence > session.sequence {
            return Ok(CompatGatewayResumeResult::InvalidSequence);
        }

        session.sequence = session.sequence.max(client_sequence);
        self.update_sequence(&session.session_id, session.sequence)
            .await?;

        Ok(CompatGatewayResumeResult::Resumed(session))
    }

    async fn append_replay_event(
        &self,
        event: CompatGatewayReplayEvent,
    ) -> Result<(), CompatGatewayError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO compat_gateway_replay_events (
                    session_id, sequence, event_type, payload
                )
                VALUES ($1, $2, $3, $4::jsonb)
                ON CONFLICT (session_id, sequence) DO UPDATE
                SET event_type = EXCLUDED.event_type,
                    payload = EXCLUDED.payload
                "#,
                vec![
                    Value::from(event.session_id),
                    Value::from(event.sequence),
                    Value::from(event.event_type),
                    Value::from(event.payload.to_string()),
                ],
            ))
            .await
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        Ok(())
    }

    async fn list_replay_events_after(
        &self,
        session_id: &str,
        sequence: i64,
        limit: u32,
    ) -> Result<Vec<CompatGatewayReplayEvent>, CompatGatewayError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT session_id, sequence, event_type, payload::text
                FROM compat_gateway_replay_events
                WHERE session_id = $1
                  AND sequence > $2
                ORDER BY sequence ASC
                LIMIT $3
                "#,
                vec![
                    Value::from(session_id.to_owned()),
                    Value::from(sequence),
                    Value::from(limit as i64),
                ],
            ))
            .await
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        rows.into_iter().map(replay_event_from_row).collect()
    }
}

impl PostgresCompatGatewaySessionStore {
    async fn get_active_session(
        &self,
        session_id: &str,
    ) -> Result<Option<CompatGatewaySession>, CompatGatewayError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT session_id, application_id::text, organization_id::text,
                       bot_user_id::text, sequence, intents
                FROM compat_gateway_sessions
                WHERE session_id = $1
                  AND expires_at > now()
                "#,
                vec![Value::from(session_id.to_owned())],
            ))
            .await
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        row.map(session_from_row).transpose()
    }
}

fn session_values(session: &CompatGatewaySession) -> Vec<Value> {
    vec![
        Value::from(session.session_id.clone()),
        Value::from(session.application_id.to_string()),
        Value::from(session.organization_id.to_string()),
        Value::from(session.bot_user_id.to_string()),
        Value::from(session.sequence),
        Value::from(session.intents as i64),
    ]
}

fn session_from_row(row: sea_orm::QueryResult) -> Result<CompatGatewaySession, CompatGatewayError> {
    let intents = row
        .try_get::<i64>("", "intents")
        .map_err(|_| CompatGatewayError::StoreUnavailable)?;
    if intents < 0 {
        return Err(CompatGatewayError::StoreUnavailable);
    }

    Ok(CompatGatewaySession {
        session_id: row_string(&row, "session_id")?,
        application_id: parse_uuid(&row_string(&row, "application_id")?)?,
        organization_id: parse_uuid(&row_string(&row, "organization_id")?)?,
        bot_user_id: parse_uuid(&row_string(&row, "bot_user_id")?)?,
        sequence: row
            .try_get::<i64>("", "sequence")
            .map_err(|_| CompatGatewayError::StoreUnavailable)?,
        intents: intents as u64,
    })
}

fn row_string(row: &sea_orm::QueryResult, column: &str) -> Result<String, CompatGatewayError> {
    row.try_get::<String>("", column)
        .map_err(|_| CompatGatewayError::StoreUnavailable)
}

fn parse_uuid(value: &str) -> Result<Uuid, CompatGatewayError> {
    Uuid::parse_str(value).map_err(|_| CompatGatewayError::StoreUnavailable)
}

fn replay_event_from_row(
    row: sea_orm::QueryResult,
) -> Result<CompatGatewayReplayEvent, CompatGatewayError> {
    let payload = row_string(&row, "payload")?;
    let payload =
        serde_json::from_str(&payload).map_err(|_| CompatGatewayError::StoreUnavailable)?;

    Ok(CompatGatewayReplayEvent {
        session_id: row_string(&row, "session_id")?,
        sequence: row
            .try_get::<i64>("", "sequence")
            .map_err(|_| CompatGatewayError::StoreUnavailable)?,
        event_type: row_string(&row, "event_type")?,
        payload,
    })
}
