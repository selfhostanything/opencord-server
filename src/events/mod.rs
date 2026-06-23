use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::observability::TraceContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KafkaTopic {
    ChatEventsV1,
    RealtimeEventsV1,
    NotificationJobsV1,
    WebhookJobsV1,
    CalendarJobsV1,
    SearchJobsV1,
    ExportJobsV1,
    RetentionJobsV1,
    MediaJobsV1,
    UsageJobsV1,
    DeadLetterV1,
}

impl KafkaTopic {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChatEventsV1 => "opencord.events.chat.v1",
            Self::RealtimeEventsV1 => "opencord.events.realtime.v1",
            Self::NotificationJobsV1 => "opencord.jobs.notifications.v1",
            Self::WebhookJobsV1 => "opencord.jobs.webhooks.v1",
            Self::CalendarJobsV1 => "opencord.jobs.calendar.v1",
            Self::SearchJobsV1 => "opencord.jobs.search.v1",
            Self::ExportJobsV1 => "opencord.jobs.exports.v1",
            Self::RetentionJobsV1 => "opencord.jobs.retention.v1",
            Self::MediaJobsV1 => "opencord.jobs.media.v1",
            Self::UsageJobsV1 => "opencord.jobs.usage.v1",
            Self::DeadLetterV1 => "opencord.dlq.v1",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub topic: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub schema_version: u16,
    pub organization_id: String,
    pub partition_key: String,
    pub occurred_at: DateTime<Utc>,
    pub traceparent: Option<String>,
    pub idempotency_key: String,
    pub payload: Value,
}

impl EventEnvelope {
    pub fn for_tenant(
        event_type: impl Into<String>,
        organization_id: impl Into<String>,
        partition_key: impl Into<String>,
        idempotency_subject: impl Into<String>,
        payload: Value,
    ) -> Self {
        let event_type = event_type.into();
        let idempotency_subject = idempotency_subject.into();

        Self {
            event_id: format!("evt_{}", Uuid::now_v7()),
            topic: topic_for_event_type(&event_type).as_str().to_owned(),
            event_type: event_type.clone(),
            schema_version: 1,
            organization_id: organization_id.into(),
            partition_key: partition_key.into(),
            occurred_at: Utc::now(),
            traceparent: None,
            idempotency_key: format!("{event_type}:{idempotency_subject}"),
            payload,
        }
    }

    pub fn with_trace_context(mut self, trace_context: &TraceContext) -> Self {
        self.traceparent = Some(trace_context.traceparent.clone());
        self
    }
}

fn topic_for_event_type(event_type: &str) -> KafkaTopic {
    if event_type.starts_with("message.") {
        KafkaTopic::ChatEventsV1
    } else {
        KafkaTopic::RealtimeEventsV1
    }
}
