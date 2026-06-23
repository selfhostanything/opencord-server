use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rskafka::client::ClientBuilder;
use rskafka::client::partition::{Compression, OffsetAt, PartitionClient, UnknownTopicHandling};
use rskafka::record::Record;

use crate::events::EventEnvelope;
use crate::queue::{ConsumerGroupName, QueueConsumer, QueueError, QueueProducer};

pub struct KafkaQueueProducer {
    bootstrap_servers: Vec<String>,
}

impl KafkaQueueProducer {
    pub async fn connect(bootstrap_servers: &[String]) -> Result<Self, QueueError> {
        ClientBuilder::new(bootstrap_servers.to_vec())
            .client_id("opencord-producer")
            .build()
            .await
            .map_err(|error| QueueError::new(error.to_string()))?;

        Ok(Self {
            bootstrap_servers: bootstrap_servers.to_vec(),
        })
    }
}

#[async_trait]
impl QueueProducer for KafkaQueueProducer {
    async fn publish(&self, envelope: EventEnvelope) -> Result<(), QueueError> {
        let client = ClientBuilder::new(self.bootstrap_servers.clone())
            .client_id("opencord-producer")
            .build()
            .await
            .map_err(|error| QueueError::new(error.to_string()))?;
        let partition = client
            .partition_client(envelope.topic.clone(), 0, UnknownTopicHandling::Retry)
            .await
            .map_err(|error| QueueError::new(error.to_string()))?;
        let payload = serde_json::to_vec(&envelope)?;

        partition
            .produce(
                vec![Record {
                    key: Some(envelope.partition_key.into_bytes()),
                    value: Some(payload),
                    headers: Default::default(),
                    timestamp: Utc::now(),
                }],
                Compression::NoCompression,
            )
            .await
            .map_err(|error| QueueError::new(error.to_string()))?;

        Ok(())
    }
}

pub struct KafkaQueueConsumer {
    partitions: Vec<Arc<PartitionClient>>,
    next_offsets: Mutex<Vec<i64>>,
}

impl KafkaQueueConsumer {
    pub async fn subscribe(
        bootstrap_servers: &[String],
        consumer_group: &ConsumerGroupName,
        topics: &[&str],
    ) -> Result<Self, QueueError> {
        let client = ClientBuilder::new(bootstrap_servers.to_vec())
            .client_id(consumer_group.as_str().to_owned())
            .build()
            .await
            .map_err(|error| QueueError::new(error.to_string()))?;

        let mut partitions = Vec::with_capacity(topics.len());
        let mut next_offsets = Vec::with_capacity(topics.len());
        for topic in topics {
            let partition = client
                .partition_client((*topic).to_owned(), 0, UnknownTopicHandling::Retry)
                .await
                .map_err(|error| QueueError::new(error.to_string()))?;
            let offset = partition
                .get_offset(OffsetAt::Earliest)
                .await
                .map_err(|error| QueueError::new(error.to_string()))?;
            partitions.push(Arc::new(partition));
            next_offsets.push(offset);
        }

        Ok(Self {
            partitions,
            next_offsets: Mutex::new(next_offsets),
        })
    }
}

#[async_trait]
impl QueueConsumer for KafkaQueueConsumer {
    async fn next(&self) -> Result<Option<EventEnvelope>, QueueError> {
        let snapshot_offsets = self
            .next_offsets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        for (index, partition) in self.partitions.iter().enumerate() {
            let offset = snapshot_offsets[index];
            let (records, _high_watermark) = partition
                .fetch_records(offset, 1..1_048_576, 250)
                .await
                .map_err(|error| QueueError::new(error.to_string()))?;

            if let Some(record) = records.into_iter().next() {
                let Some(value) = record.record.value else {
                    continue;
                };
                let envelope = serde_json::from_slice(&value)?;
                let mut next_offsets = self
                    .next_offsets
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                next_offsets[index] = record.offset + 1;
                return Ok(Some(envelope));
            }
        }

        Ok(None)
    }
}
