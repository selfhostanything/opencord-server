use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::domain::ids;
use crate::events::EventEnvelope;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RealtimeScope {
    pub space_id: Option<String>,
    pub channel_id: Option<String>,
}

impl From<RealtimeEvent> for EventEnvelope {
    fn from(event: RealtimeEvent) -> Self {
        EventEnvelope::for_tenant(
            event.event_type.clone(),
            event.organization_id.clone(),
            realtime_partition_key(&event),
            event.id.clone(),
            serde_json::to_value(event).unwrap_or(Value::Null),
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RealtimeEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub organization_id: String,
    pub scope: RealtimeScope,
    pub occurred_at: String,
    pub data: Value,
}

impl RealtimeEvent {
    pub fn space(event_type: &str, organization_id: Uuid, space_id: Uuid, data: Value) -> Self {
        Self {
            id: format!("evt_{}", ids::new_uuid_v7()),
            event_type: event_type.to_owned(),
            organization_id: organization_id.to_string(),
            scope: RealtimeScope {
                space_id: Some(space_id.to_string()),
                channel_id: None,
            },
            occurred_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            data,
        }
    }

    pub fn channel(
        event_type: &str,
        organization_id: Uuid,
        space_id: Uuid,
        channel_id: Uuid,
        data: Value,
    ) -> Self {
        Self {
            id: format!("evt_{}", ids::new_uuid_v7()),
            event_type: event_type.to_owned(),
            organization_id: organization_id.to_string(),
            scope: RealtimeScope {
                space_id: Some(space_id.to_string()),
                channel_id: Some(channel_id.to_string()),
            },
            occurred_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            data,
        }
    }
}

#[derive(Clone)]
pub struct RealtimeHub {
    sender: broadcast::Sender<RealtimeEvent>,
}

impl Default for RealtimeHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }
}

impl RealtimeHub {
    pub fn publish(&self, event: RealtimeEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
        self.sender.subscribe()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeSubscriber {
    pub user_id: Uuid,
    pub visible_channel_ids: BTreeSet<Uuid>,
}

impl RealtimeSubscriber {
    pub fn can_receive(&self, event: &RealtimeEvent) -> bool {
        let Some(channel_id) = event
            .scope
            .channel_id
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
        else {
            return true;
        };

        self.visible_channel_ids.contains(&channel_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealtimeReplayEntry {
    pub session_id: String,
    pub sequence_number: i64,
    pub event: RealtimeEvent,
}

#[async_trait]
pub trait RealtimeReplayStore: Send + Sync {
    async fn append(&self, session_id: &str, event: RealtimeEvent) -> i64;

    async fn read_after(&self, session_id: &str, after_sequence: i64) -> Vec<RealtimeReplayEntry>;
}

#[derive(Default)]
pub struct InMemoryRealtimeReplayStore {
    entries_by_session: Mutex<BTreeMap<String, Vec<RealtimeEvent>>>,
}

#[async_trait]
impl RealtimeReplayStore for InMemoryRealtimeReplayStore {
    async fn append(&self, session_id: &str, event: RealtimeEvent) -> i64 {
        let mut entries = self
            .entries_by_session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_entries = entries.entry(session_id.to_owned()).or_default();
        session_entries.push(event);
        session_entries.len() as i64
    }

    async fn read_after(&self, session_id: &str, after_sequence: i64) -> Vec<RealtimeReplayEntry> {
        let entries = self
            .entries_by_session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        entries
            .get(session_id)
            .into_iter()
            .flat_map(|events| events.iter().enumerate())
            .filter_map(|(index, event)| {
                let sequence_number = index as i64 + 1;
                (sequence_number > after_sequence).then(|| RealtimeReplayEntry {
                    session_id: session_id.to_owned(),
                    sequence_number,
                    event: event.clone(),
                })
            })
            .collect()
    }
}

pub fn filter_realtime_event_for_subscriber(
    event: &RealtimeEvent,
    subscriber: &RealtimeSubscriber,
) -> Option<RealtimeEvent> {
    subscriber.can_receive(event).then(|| event.clone())
}

fn realtime_partition_key(event: &RealtimeEvent) -> String {
    event
        .scope
        .channel_id
        .clone()
        .or_else(|| event.scope.space_id.clone())
        .unwrap_or_else(|| event.organization_id.clone())
}
