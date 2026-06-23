use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::events::EventEnvelope;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_backoff_ms: u64,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, initial_backoff_ms: u64) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            initial_backoff_ms,
        }
    }

    pub fn decide(&self, attempt: u32) -> RetryDecision {
        if attempt >= self.max_attempts {
            RetryDecision::DeadLetter
        } else {
            let multiplier = 2_u64.saturating_pow(attempt.saturating_sub(1));
            RetryDecision::Retry {
                backoff_ms: self.initial_backoff_ms.saturating_mul(multiplier),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    Retry { backoff_ms: u64 },
    DeadLetter,
}

#[derive(Default)]
pub struct InMemoryIdempotencyGuard {
    claimed_keys: Mutex<BTreeSet<String>>,
}

impl InMemoryIdempotencyGuard {
    pub fn claim(&self, idempotency_key: &str) -> bool {
        let mut claimed_keys = self
            .claimed_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        claimed_keys.insert(idempotency_key.to_owned())
    }
}

#[async_trait]
pub trait JobHandler: Send + Sync {
    async fn handle(&self, envelope: &EventEnvelope) -> Result<(), JobError>;
}

pub struct FnJobHandler<F> {
    handler: F,
}

impl<F> FnJobHandler<F> {
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl<F> JobHandler for FnJobHandler<F>
where
    F: Send
        + Sync
        + Fn(&EventEnvelope) -> Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>,
{
    async fn handle(&self, envelope: &EventEnvelope) -> Result<(), JobError> {
        (self.handler)(envelope).await
    }
}

#[derive(Debug)]
pub struct JobError {
    message: String,
}

impl JobError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JobError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobOutcome {
    Completed,
    Duplicate,
    Retry { backoff_ms: u64 },
    DeadLetter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobLogFields {
    pub job_id: String,
    pub event_id: String,
    pub organization_id: String,
    pub attempt: u32,
    pub idempotency_key: String,
}

impl JobLogFields {
    pub fn from_envelope(envelope: &EventEnvelope, attempt: u32) -> Self {
        Self {
            job_id: envelope.event_type.clone(),
            event_id: envelope.event_id.clone(),
            organization_id: envelope.organization_id.clone(),
            attempt,
            idempotency_key: envelope.idempotency_key.clone(),
        }
    }
}

#[async_trait]
pub trait DeadLetterSink: Send + Sync {
    async fn record(&self, envelope: &EventEnvelope, attempt: u32, reason: &str);
}

#[derive(Default)]
pub struct InMemoryDeadLetterSink {
    entries: Mutex<Vec<DeadLetterEntry>>,
}

impl InMemoryDeadLetterSink {
    pub fn entries(&self) -> Vec<DeadLetterEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadLetterEntry {
    pub event_id: String,
    pub idempotency_key: String,
    pub attempt: u32,
    pub reason: String,
}

#[async_trait]
impl DeadLetterSink for InMemoryDeadLetterSink {
    async fn record(&self, envelope: &EventEnvelope, attempt: u32, reason: &str) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.push(DeadLetterEntry {
            event_id: envelope.event_id.clone(),
            idempotency_key: envelope.idempotency_key.clone(),
            attempt,
            reason: reason.to_owned(),
        });
    }
}

pub async fn process_job_once(
    envelope: &EventEnvelope,
    attempt: u32,
    handler: &dyn JobHandler,
    guard: &InMemoryIdempotencyGuard,
    retry_policy: RetryPolicy,
) -> JobOutcome {
    if !guard.claim(&envelope.idempotency_key) {
        return JobOutcome::Duplicate;
    }

    match handler.handle(envelope).await {
        Ok(()) => JobOutcome::Completed,
        Err(_) => match retry_policy.decide(attempt) {
            RetryDecision::Retry { backoff_ms } => JobOutcome::Retry { backoff_ms },
            RetryDecision::DeadLetter => JobOutcome::DeadLetter,
        },
    }
}

pub async fn process_job_with_dead_letter(
    envelope: &EventEnvelope,
    attempt: u32,
    handler: &dyn JobHandler,
    guard: &InMemoryIdempotencyGuard,
    retry_policy: RetryPolicy,
    dead_letters: Arc<dyn DeadLetterSink>,
) -> JobOutcome {
    if !guard.claim(&envelope.idempotency_key) {
        return JobOutcome::Duplicate;
    }

    match handler.handle(envelope).await {
        Ok(()) => JobOutcome::Completed,
        Err(error) => match retry_policy.decide(attempt) {
            RetryDecision::Retry { backoff_ms } => JobOutcome::Retry { backoff_ms },
            RetryDecision::DeadLetter => {
                dead_letters
                    .record(envelope, attempt, &error.to_string())
                    .await;
                JobOutcome::DeadLetter
            }
        },
    }
}
