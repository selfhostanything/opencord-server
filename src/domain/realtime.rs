use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RealtimeScope {
    pub space_id: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
