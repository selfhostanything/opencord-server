use std::sync::Arc;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::attachment::{AttachmentError, AttachmentStore};
use crate::domain::audit::{AuditError, AuditStore};
use crate::domain::ids;
use crate::domain::message::{MessageError, MessageStore};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPolicy {
    pub organization_id: Uuid,
    pub messages_retain_days: Option<i64>,
    pub files_retain_days: Option<i64>,
    pub audit_logs_retain_days: Option<i64>,
    pub deleted_message_purge_days: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionRun {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub dry_run: bool,
    pub messages_purged: usize,
    pub files_purged: usize,
    pub audit_events_purged: usize,
    pub ran_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionRunSummary {
    pub organizations_scanned: usize,
    pub messages_purged: usize,
    pub files_purged: usize,
    pub audit_events_purged: usize,
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum RetentionError {
    InvalidInput(&'static str),
    NotFound,
    StoreUnavailable,
}

impl RetentionError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::NotFound => "retention_policy_not_found",
            Self::StoreUnavailable => "retention_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::NotFound => "retention policy was not found",
            Self::StoreUnavailable => "retention store is unavailable",
        }
    }
}

impl From<MessageError> for RetentionError {
    fn from(error: MessageError) -> Self {
        match error {
            MessageError::InvalidInput(message) => Self::InvalidInput(message),
            MessageError::NotFound | MessageError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

impl From<AttachmentError> for RetentionError {
    fn from(error: AttachmentError) -> Self {
        match error {
            AttachmentError::InvalidInput(message) => Self::InvalidInput(message),
            AttachmentError::NotFound | AttachmentError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

impl From<AuditError> for RetentionError {
    fn from(error: AuditError) -> Self {
        match error {
            AuditError::InvalidInput(message) => Self::InvalidInput(message),
            AuditError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

#[async_trait::async_trait]
pub trait RetentionStore: Send + Sync {
    async fn upsert_policy(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionError>;
    async fn get_policy(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionError>;
    async fn list_policies(&self) -> Result<Vec<RetentionPolicy>, RetentionError>;
    async fn record_run(&self, run: RetentionRun) -> Result<(), RetentionError>;
    async fn list_runs_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<RetentionRun>, RetentionError>;
}

#[derive(Clone)]
pub struct RetentionService {
    store: Arc<dyn RetentionStore>,
}

impl RetentionService {
    pub fn new(store: Arc<dyn RetentionStore>) -> Self {
        Self { store }
    }

    pub async fn upsert_policy(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionError> {
        self.store.upsert_policy(normalize_policy(policy)?).await
    }

    pub async fn get_policy(
        &self,
        organization_id: Uuid,
    ) -> Result<RetentionPolicy, RetentionError> {
        self.store
            .get_policy(organization_id)
            .await?
            .ok_or(RetentionError::NotFound)
    }
}

#[derive(Clone)]
pub struct RetentionWorker {
    retention: Arc<dyn RetentionStore>,
    messages: Arc<dyn MessageStore>,
    attachments: Arc<dyn AttachmentStore>,
    audit: Arc<dyn AuditStore>,
}

impl RetentionWorker {
    pub fn new(
        retention: Arc<dyn RetentionStore>,
        messages: Arc<dyn MessageStore>,
        attachments: Arc<dyn AttachmentStore>,
        audit: Arc<dyn AuditStore>,
    ) -> Self {
        Self {
            retention,
            messages,
            attachments,
            audit,
        }
    }

    pub async fn run_once(&self, dry_run: bool) -> Result<RetentionRunSummary, RetentionError> {
        self.run_once_at(
            Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            dry_run,
        )
        .await
    }

    pub async fn run_once_at(
        &self,
        now: String,
        dry_run: bool,
    ) -> Result<RetentionRunSummary, RetentionError> {
        let now = parse_time(now, "retention run timestamp must be RFC3339")?;
        let ran_at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let policies = self.retention.list_policies().await?;
        let mut summary = RetentionRunSummary {
            organizations_scanned: policies.len(),
            messages_purged: 0,
            files_purged: 0,
            audit_events_purged: 0,
            dry_run,
        };

        for policy in policies {
            let policy = normalize_policy(policy)?;
            let messages_cutoff = cutoff(now, policy.messages_retain_days);
            let deleted_cutoff = cutoff(now, policy.deleted_message_purge_days);
            let files_cutoff = cutoff(now, policy.files_retain_days);
            let audit_cutoff = cutoff(now, policy.audit_logs_retain_days);

            let messages_purged = self
                .messages
                .purge_for_retention(
                    policy.organization_id,
                    messages_cutoff,
                    deleted_cutoff,
                    dry_run,
                )
                .await?;
            let files_purged = self
                .attachments
                .purge_for_retention(policy.organization_id, files_cutoff, dry_run)
                .await?;
            let audit_events_purged = self
                .audit
                .purge_for_retention(policy.organization_id, audit_cutoff, dry_run)
                .await?;

            summary.messages_purged += messages_purged;
            summary.files_purged += files_purged;
            summary.audit_events_purged += audit_events_purged;

            self.retention
                .record_run(RetentionRun {
                    id: ids::new_uuid_v7(),
                    organization_id: policy.organization_id,
                    dry_run,
                    messages_purged,
                    files_purged,
                    audit_events_purged,
                    ran_at: ran_at.clone(),
                })
                .await?;
        }

        Ok(summary)
    }
}

fn normalize_policy(policy: RetentionPolicy) -> Result<RetentionPolicy, RetentionError> {
    Ok(RetentionPolicy {
        organization_id: policy.organization_id,
        messages_retain_days: normalize_days(policy.messages_retain_days, "messages_retain_days")?,
        files_retain_days: normalize_days(policy.files_retain_days, "files_retain_days")?,
        audit_logs_retain_days: normalize_days(
            policy.audit_logs_retain_days,
            "audit_logs_retain_days",
        )?,
        deleted_message_purge_days: normalize_days(
            policy.deleted_message_purge_days,
            "deleted_message_purge_days",
        )?,
    })
}

fn normalize_days(
    value: Option<i64>,
    field_name: &'static str,
) -> Result<Option<i64>, RetentionError> {
    match value {
        Some(days) if (1..=3650).contains(&days) => Ok(Some(days)),
        Some(_) => Err(RetentionError::InvalidInput(match field_name {
            "messages_retain_days" => "messages_retain_days must be between 1 and 3650",
            "files_retain_days" => "files_retain_days must be between 1 and 3650",
            "audit_logs_retain_days" => "audit_logs_retain_days must be between 1 and 3650",
            "deleted_message_purge_days" => "deleted_message_purge_days must be between 1 and 3650",
            _ => "retention days must be between 1 and 3650",
        })),
        None => Ok(None),
    }
}

fn cutoff(now: DateTime<Utc>, days: Option<i64>) -> Option<String> {
    days.map(|days| (now - Duration::days(days)).to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn parse_time(value: String, message: &'static str) -> Result<DateTime<Utc>, RetentionError> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| RetentionError::InvalidInput(message))
}
