use std::time::Duration;

use opencord_server::events::{EventEnvelope, KafkaTopic};
use opencord_server::queue::kafka::{KafkaQueueConsumer, KafkaQueueProducer};
use opencord_server::queue::{ConsumerGroupName, QueueConsumer, QueueProducer};
use serde_json::json;

#[tokio::test]
async fn kafka_producer_and_consumer_exchange_a_test_event() {
    if std::env::var("OPENCORD_KAFKA_SMOKE").ok().as_deref() != Some("1") {
        eprintln!("set OPENCORD_KAFKA_SMOKE=1 to run the local Kafka smoke test");
        return;
    }

    let brokers = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
        .unwrap_or_else(|_| "localhost:29092".to_owned())
        .split(',')
        .map(str::trim)
        .filter(|broker| !broker.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let event_subject = format!("smoke_{}", uuid::Uuid::now_v7());
    let envelope = EventEnvelope::for_tenant(
        "message.created",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        &event_subject,
        json!({ "smoke_id": event_subject }),
    );

    let producer = KafkaQueueProducer::connect(&brokers)
        .await
        .expect("connect Kafka producer");
    let consumer = KafkaQueueConsumer::subscribe(
        &brokers,
        &ConsumerGroupName::for_worker_role(&format!("smoke-{event_subject}")),
        &[KafkaTopic::ChatEventsV1.as_str()],
    )
    .await
    .expect("connect Kafka consumer");

    producer
        .publish(envelope.clone())
        .await
        .expect("publish smoke event");

    for _ in 0..30 {
        if let Some(received) = consumer.next().await.expect("consume smoke event")
            && received.idempotency_key == envelope.idempotency_key
        {
            assert_eq!(received.payload, envelope.payload);
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    panic!("Kafka smoke event was not consumed");
}
