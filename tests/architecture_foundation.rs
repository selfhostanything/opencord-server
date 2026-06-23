use async_trait::async_trait;
use chrono::{Duration, Utc};
use opencord_server::config::{LogFormat, RuntimeConfig};
use opencord_server::domain::metrics::MediaMetrics;
use opencord_server::domain::realtime::{
    InMemoryRealtimeReplayStore, RealtimeEvent, RealtimeReplayStore, RealtimeSubscriber,
    filter_realtime_event_for_subscriber,
};
use opencord_server::events::{EventEnvelope, KafkaTopic};
use opencord_server::jobs::{
    InMemoryDeadLetterSink, InMemoryIdempotencyGuard, JobError, JobHandler, JobLogFields,
    JobOutcome, RetryDecision, RetryPolicy, process_job_once, process_job_with_dead_letter,
};
use opencord_server::observability::{TraceContext, otel_export_enabled};
use opencord_server::queue::ConsumerGroupName;
use opencord_server::scylla::{
    InMemoryPresenceStore, PresenceHeartbeat, PresenceStatus, PresenceStore, ScyllaTable,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn config_defaults_cover_scale_dependencies_and_reject_blank_values() {
    let config = RuntimeConfig::from_env_pairs(&[]).expect("default config is valid");

    assert_eq!(config.kafka.bootstrap_servers, vec!["localhost:29092"]);
    assert_eq!(config.scylla.contact_points, vec!["localhost:9042"]);
    assert_eq!(config.valkey.url, "redis://localhost:6379/0");
    assert_eq!(config.object_storage.endpoint, "http://localhost:9000");
    assert!(!config.otel.enabled);
    assert_eq!(config.log.format, LogFormat::Text);
    assert!(config.metrics.prometheus_enabled);

    assert!(RuntimeConfig::from_env_pairs(&[("KAFKA_BOOTSTRAP_SERVERS", "")]).is_err());
    assert!(RuntimeConfig::from_env_pairs(&[("SCYLLA_CONTACT_POINTS", "scylla")]).is_err());
    assert!(RuntimeConfig::from_env_pairs(&[("OPENCORD_LOG_FORMAT", "xml")]).is_err());
    assert_eq!(
        RuntimeConfig::from_env_pairs(&[("OPENCORD_LOG_FORMAT", "json")])
            .expect("json log format is valid")
            .log
            .format,
        LogFormat::Json,
    );
}

#[test]
fn kafka_event_envelope_uses_uuid_v7_ids_tenant_scope_and_idempotency_key() {
    let trace_context =
        TraceContext::new("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00")
            .expect("trace context is valid");
    let envelope = EventEnvelope::for_tenant(
        "message.created",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "msg_01973f83-f22a-73ba-ae76-5a045c52fc97",
        json!({ "message_id": "msg_01973f83-f22a-73ba-ae76-5a045c52fc97" }),
    )
    .with_trace_context(&trace_context);

    let uuid = Uuid::parse_str(
        envelope
            .event_id
            .strip_prefix("evt_")
            .expect("event IDs carry the evt_ prefix"),
    )
    .expect("event ID suffix is a UUID");

    assert_eq!(uuid.get_version_num(), 7);
    assert_eq!(envelope.topic, KafkaTopic::ChatEventsV1.as_str());
    assert_eq!(envelope.schema_version, 1);
    assert_eq!(
        envelope.idempotency_key,
        "message.created:msg_01973f83-f22a-73ba-ae76-5a045c52fc97",
    );
    assert_eq!(
        envelope.organization_id,
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96"
    );
    assert_eq!(
        envelope.traceparent,
        Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".to_owned())
    );
}

#[test]
fn worker_retry_policy_and_idempotency_guard_prevent_duplicate_side_effects() {
    let policy = RetryPolicy::new(3, 250);

    assert_eq!(policy.decide(1), RetryDecision::Retry { backoff_ms: 250 });
    assert_eq!(policy.decide(2), RetryDecision::Retry { backoff_ms: 500 });
    assert_eq!(policy.decide(3), RetryDecision::DeadLetter);

    let guard = InMemoryIdempotencyGuard::default();
    assert!(guard.claim("message.created:msg_1"));
    assert!(!guard.claim("message.created:msg_1"));
    assert!(guard.claim("message.created:msg_2"));
}

#[tokio::test]
async fn worker_handler_retries_dead_letters_and_skips_duplicate_jobs() {
    let envelope = EventEnvelope::for_tenant(
        "notification.deliver",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "notification_1",
        json!({ "notification_id": "notification_1" }),
    );
    let retry_policy = RetryPolicy::new(2, 100);
    let failing_handler = FailingJobHandler;

    let retry_guard = InMemoryIdempotencyGuard::default();
    assert_eq!(
        process_job_once(&envelope, 1, &failing_handler, &retry_guard, retry_policy).await,
        JobOutcome::Retry { backoff_ms: 100 },
    );
    assert_eq!(
        process_job_once(&envelope, 1, &failing_handler, &retry_guard, retry_policy).await,
        JobOutcome::Duplicate,
    );

    let dead_letter_guard = InMemoryIdempotencyGuard::default();
    let dead_letters = Arc::new(InMemoryDeadLetterSink::default());
    assert_eq!(
        process_job_with_dead_letter(
            &envelope,
            2,
            &failing_handler,
            &dead_letter_guard,
            retry_policy,
            dead_letters.clone(),
        )
        .await,
        JobOutcome::DeadLetter,
    );
    assert_eq!(dead_letters.entries().len(), 1);
    assert_eq!(
        dead_letters.entries()[0].idempotency_key,
        envelope.idempotency_key
    );
    assert_eq!(dead_letters.entries()[0].attempt, 2);
}

#[test]
fn job_log_fields_include_event_tenant_and_attempt_context() {
    let envelope = EventEnvelope::for_tenant(
        "webhook.deliver",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "org_01973f83-f22a-73ba-ae76-5a045c52fc96",
        "webhook_1",
        json!({ "webhook_id": "webhook_1" }),
    );

    let fields = JobLogFields::from_envelope(&envelope, 3);

    assert_eq!(fields.job_id, "webhook.deliver");
    assert_eq!(fields.event_id, envelope.event_id);
    assert_eq!(fields.organization_id, envelope.organization_id);
    assert_eq!(fields.attempt, 3);
    assert_eq!(fields.idempotency_key, envelope.idempotency_key);
}

#[tokio::test]
async fn realtime_scale_path_filters_private_events_and_replays_by_sequence() {
    let organization_id = Uuid::now_v7();
    let space_id = Uuid::now_v7();
    let public_channel_id = Uuid::now_v7();
    let private_channel_id = Uuid::now_v7();
    let subscriber = RealtimeSubscriber {
        user_id: Uuid::now_v7(),
        visible_channel_ids: BTreeSet::from([public_channel_id]),
    };
    let public_event = RealtimeEvent::channel(
        "message.created",
        organization_id,
        space_id,
        public_channel_id,
        json!({ "message_id": "public" }),
    );
    let private_event = RealtimeEvent::channel(
        "message.created",
        organization_id,
        space_id,
        private_channel_id,
        json!({ "message_id": "private" }),
    );

    assert!(filter_realtime_event_for_subscriber(&public_event, &subscriber).is_some());
    assert!(filter_realtime_event_for_subscriber(&private_event, &subscriber).is_none());

    let envelope: EventEnvelope = public_event.clone().into();
    assert_eq!(envelope.topic, KafkaTopic::ChatEventsV1.as_str());
    assert_eq!(envelope.partition_key, public_channel_id.to_string());

    let replay = InMemoryRealtimeReplayStore::default();
    assert_eq!(replay.append("session-1", public_event.clone()).await, 1);
    assert_eq!(replay.append("session-1", private_event.clone()).await, 2);

    let replayed = replay.read_after("session-1", 1).await;
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].sequence_number, 2);
    assert_eq!(replayed[0].event.id, private_event.id);
}

struct FailingJobHandler;

#[async_trait]
impl JobHandler for FailingJobHandler {
    async fn handle(&self, _envelope: &EventEnvelope) -> Result<(), JobError> {
        Err(JobError::new("temporary failure"))
    }
}

#[test]
fn metrics_renderer_exposes_kafka_job_and_scylla_counters() {
    let metrics = MediaMetrics::default();
    metrics.record_kafka_produced();
    metrics.record_kafka_consumed();
    metrics.record_kafka_consumer_lag("opencord-worker-notifications", 7);
    metrics.record_job_completed();
    metrics.record_job_retried();
    metrics.record_job_dead_lettered();
    metrics.record_scylla_read();
    metrics.record_scylla_write();

    let rendered = metrics.render_prometheus();
    assert!(rendered.contains("opencord_kafka_produced_total 1"));
    assert!(rendered.contains("opencord_kafka_consumed_total 1"));
    assert!(
        rendered.contains("opencord_kafka_consumer_lag{group=\"opencord-worker-notifications\"} 7")
    );
    assert!(rendered.contains("opencord_jobs_completed_total 1"));
    assert!(rendered.contains("opencord_jobs_retried_total 1"));
    assert!(rendered.contains("opencord_jobs_dead_lettered_total 1"));
    assert!(rendered.contains("opencord_scylla_reads_total 1"));
    assert!(rendered.contains("opencord_scylla_writes_total 1"));
}

#[test]
fn consumer_group_names_are_stable_per_worker_role() {
    assert_eq!(
        ConsumerGroupName::for_worker_role("notifications").as_str(),
        "opencord-worker-notifications",
    );
    assert_eq!(
        ConsumerGroupName::for_worker_role("calendar.sync").as_str(),
        "opencord-worker-calendar-sync",
    );
}

#[test]
fn otel_export_is_optional_and_requires_an_endpoint_when_enabled() {
    let disabled = RuntimeConfig::from_env_pairs(&[]).expect("default config is valid");
    assert!(!otel_export_enabled(&disabled));

    let enabled = RuntimeConfig::from_env_pairs(&[
        ("OPENCORD_OTEL_ENABLED", "true"),
        ("OPENCORD_OTEL_ENDPOINT", "http://otel-collector:4317"),
    ])
    .expect("otel config is valid");
    assert!(otel_export_enabled(&enabled));

    assert_eq!(
        TraceContext::new("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00")
            .expect("traceparent is present")
            .traceparent,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00",
    );
    assert!(TraceContext::new("").is_none());
    assert!(TraceContext::new("not-a-traceparent").is_none());
    assert!(TraceContext::new("00-00000000000000000000000000000000-00f067aa0ba902b7-00").is_none());
    assert!(TraceContext::new("00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-00").is_none());
}

#[tokio::test]
async fn presence_reference_store_reads_by_partition_key_and_honors_ttl() {
    let store = InMemoryPresenceStore::default();
    let organization_id = Uuid::now_v7();
    let space_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let now = Utc::now();

    store
        .upsert(PresenceHeartbeat {
            organization_id,
            space_id,
            user_id,
            status: PresenceStatus::Online,
            observed_at: now,
            expires_at: now + Duration::seconds(60),
        })
        .await
        .expect("write presence");

    let active = store
        .list_active_by_space(organization_id, space_id, now + Duration::seconds(30))
        .await;
    let active = active.expect("read active presence");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].user_id, user_id);
    assert_eq!(ScyllaTable::PresenceBySpace.as_str(), "presence_by_space");

    let expired = store
        .list_active_by_space(organization_id, space_id, now + Duration::seconds(90))
        .await;
    let expired = expired.expect("read expired presence");
    assert!(expired.is_empty());
}
