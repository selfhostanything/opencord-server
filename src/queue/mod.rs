use async_trait::async_trait;

use crate::events::EventEnvelope;

pub mod kafka;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumerGroupName(String);

impl ConsumerGroupName {
    pub fn for_worker_role(role: &str) -> Self {
        let normalized = role
            .trim()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        Self(format!("opencord-worker-{normalized}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[async_trait]
pub trait QueueProducer: Send + Sync {
    async fn publish(&self, envelope: EventEnvelope) -> Result<(), QueueError>;
}

#[async_trait]
pub trait QueueConsumer: Send + Sync {
    async fn next(&self) -> Result<Option<EventEnvelope>, QueueError>;
}

#[derive(Debug)]
pub struct QueueError {
    message: String,
}

impl QueueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for QueueError {}

impl From<serde_json::Error> for QueueError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}
