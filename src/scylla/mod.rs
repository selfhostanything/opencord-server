use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScyllaTable {
    PresenceBySpace,
    TypingByChannel,
    GatewayEventsBySession,
    DeliveryReceiptsByMessage,
    NotificationAttemptsByUser,
    MediaQualityByRoom,
    ClientTelemetryByTenant,
    BotDispatchLogsByApplication,
}

impl ScyllaTable {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PresenceBySpace => "presence_by_space",
            Self::TypingByChannel => "typing_by_channel",
            Self::GatewayEventsBySession => "gateway_events_by_session",
            Self::DeliveryReceiptsByMessage => "delivery_receipts_by_message",
            Self::NotificationAttemptsByUser => "notification_attempts_by_user",
            Self::MediaQualityByRoom => "media_quality_by_room",
            Self::ClientTelemetryByTenant => "client_telemetry_by_tenant",
            Self::BotDispatchLogsByApplication => "bot_dispatch_logs_by_application",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceStatus {
    Online,
    Idle,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceHeartbeat {
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub user_id: Uuid,
    pub status: PresenceStatus,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait PresenceStore: Send + Sync {
    async fn upsert(&self, heartbeat: PresenceHeartbeat) -> Result<(), ScyllaStoreError>;

    async fn list_active_by_space(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Vec<PresenceHeartbeat>, ScyllaStoreError>;
}

#[derive(Debug)]
pub struct ScyllaStoreError {
    message: String,
}

impl ScyllaStoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ScyllaStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ScyllaStoreError {}

#[derive(Default)]
pub struct InMemoryPresenceStore {
    heartbeats: Mutex<Vec<PresenceHeartbeat>>,
}

#[async_trait]
impl PresenceStore for InMemoryPresenceStore {
    async fn upsert(&self, heartbeat: PresenceHeartbeat) -> Result<(), ScyllaStoreError> {
        let mut heartbeats = self
            .heartbeats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        heartbeats.retain(|candidate| {
            candidate.organization_id != heartbeat.organization_id
                || candidate.space_id != heartbeat.space_id
                || candidate.user_id != heartbeat.user_id
        });
        heartbeats.push(heartbeat);

        Ok(())
    }

    async fn list_active_by_space(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Vec<PresenceHeartbeat>, ScyllaStoreError> {
        let heartbeats = self
            .heartbeats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        Ok(heartbeats
            .iter()
            .filter(|heartbeat| {
                heartbeat.organization_id == organization_id
                    && heartbeat.space_id == space_id
                    && heartbeat.expires_at > at
            })
            .cloned()
            .collect())
    }
}

pub struct ScyllaPresenceStore {
    session: Session,
    keyspace: String,
}

impl ScyllaPresenceStore {
    pub async fn connect(
        contact_points: &[String],
        keyspace: impl Into<String>,
    ) -> Result<Self, ScyllaStoreError> {
        let mut builder = SessionBuilder::new();
        for contact_point in contact_points {
            builder = builder.known_node(contact_point);
        }
        let session = builder
            .build()
            .await
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?;

        Ok(Self {
            session,
            keyspace: validate_identifier(keyspace.into(), "keyspace")?,
        })
    }

    pub async fn bootstrap_schema(&self) -> Result<(), ScyllaStoreError> {
        self.session
            .query_unpaged(
                format!(
                    "CREATE KEYSPACE IF NOT EXISTS {} WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}",
                    self.keyspace
                ),
                (),
            )
            .await
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?;
        self.session
            .query_unpaged(
                format!(
                    "CREATE TABLE IF NOT EXISTS {}.{} (
                        organization_id text,
                        space_id text,
                        user_id text,
                        status text,
                        observed_at_ms bigint,
                        expires_at_ms bigint,
                        PRIMARY KEY ((organization_id, space_id), user_id)
                    )",
                    self.keyspace,
                    ScyllaTable::PresenceBySpace.as_str(),
                ),
                (),
            )
            .await
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl PresenceStore for ScyllaPresenceStore {
    async fn upsert(&self, heartbeat: PresenceHeartbeat) -> Result<(), ScyllaStoreError> {
        let ttl_seconds = (heartbeat.expires_at - heartbeat.observed_at)
            .num_seconds()
            .max(1) as i32;
        self.session
            .query_unpaged(
                format!(
                    "INSERT INTO {}.{} (
                        organization_id,
                        space_id,
                        user_id,
                        status,
                        observed_at_ms,
                        expires_at_ms
                    ) VALUES (?, ?, ?, ?, ?, ?) USING TTL ?",
                    self.keyspace,
                    ScyllaTable::PresenceBySpace.as_str(),
                ),
                (
                    heartbeat.organization_id.to_string(),
                    heartbeat.space_id.to_string(),
                    heartbeat.user_id.to_string(),
                    presence_status_to_str(&heartbeat.status),
                    heartbeat.observed_at.timestamp_millis(),
                    heartbeat.expires_at.timestamp_millis(),
                    ttl_seconds,
                ),
            )
            .await
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?;

        Ok(())
    }

    async fn list_active_by_space(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Vec<PresenceHeartbeat>, ScyllaStoreError> {
        let rows = self
            .session
            .query_unpaged(
                format!(
                    "SELECT user_id, status, observed_at_ms, expires_at_ms FROM {}.{} WHERE organization_id = ? AND space_id = ?",
                    self.keyspace,
                    ScyllaTable::PresenceBySpace.as_str(),
                ),
                (organization_id.to_string(), space_id.to_string()),
            )
            .await
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?
            .into_rows_result()
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?;

        let mut heartbeats = Vec::new();
        for row in rows
            .rows::<(String, String, i64, i64)>()
            .map_err(|error| ScyllaStoreError::new(error.to_string()))?
        {
            let (user_id, status, observed_at_ms, expires_at_ms) =
                row.map_err(|error| ScyllaStoreError::new(error.to_string()))?;
            let expires_at = datetime_from_millis(expires_at_ms)?;
            if expires_at <= at {
                continue;
            }

            heartbeats.push(PresenceHeartbeat {
                organization_id,
                space_id,
                user_id: Uuid::parse_str(&user_id)
                    .map_err(|error| ScyllaStoreError::new(error.to_string()))?,
                status: presence_status_from_str(&status),
                observed_at: datetime_from_millis(observed_at_ms)?,
                expires_at,
            });
        }

        Ok(heartbeats)
    }
}

fn validate_identifier(value: String, label: &str) -> Result<String, ScyllaStoreError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(ScyllaStoreError::new(format!(
            "{label} must contain only ASCII letters, numbers, and underscores",
        )));
    }

    Ok(value)
}

fn datetime_from_millis(value: i64) -> Result<DateTime<Utc>, ScyllaStoreError> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| ScyllaStoreError::new(format!("invalid timestamp millis: {value}")))
}

fn presence_status_to_str(status: &PresenceStatus) -> &'static str {
    match status {
        PresenceStatus::Online => "online",
        PresenceStatus::Idle => "idle",
        PresenceStatus::Offline => "offline",
    }
}

fn presence_status_from_str(value: &str) -> PresenceStatus {
    match value {
        "idle" => PresenceStatus::Idle,
        "offline" => PresenceStatus::Offline,
        _ => PresenceStatus::Online,
    }
}
