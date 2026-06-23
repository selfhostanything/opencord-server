use serde::{Deserialize, Serialize};

use crate::domain::retention::{RetentionPolicy, RetentionRun};

#[derive(Debug, Deserialize)]
pub struct UpsertRetentionPolicyRequest {
    pub messages_retain_days: Option<i64>,
    pub files_retain_days: Option<i64>,
    pub audit_logs_retain_days: Option<i64>,
    pub deleted_message_purge_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RetentionPolicyEnvelope {
    pub policy: RetentionPolicyResponse,
}

#[derive(Debug, Serialize)]
pub struct RetentionRunListResponse {
    pub retention_runs: Vec<RetentionRunResponse>,
}

#[derive(Debug, Serialize)]
pub struct RetentionPolicyResponse {
    pub organization_id: String,
    pub messages_retain_days: Option<i64>,
    pub files_retain_days: Option<i64>,
    pub audit_logs_retain_days: Option<i64>,
    pub deleted_message_purge_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RetentionRunResponse {
    pub id: String,
    pub organization_id: String,
    pub dry_run: bool,
    pub messages_purged: usize,
    pub files_purged: usize,
    pub audit_events_purged: usize,
    pub ran_at: String,
}

impl From<RetentionPolicy> for RetentionPolicyResponse {
    fn from(policy: RetentionPolicy) -> Self {
        Self {
            organization_id: policy.organization_id.to_string(),
            messages_retain_days: policy.messages_retain_days,
            files_retain_days: policy.files_retain_days,
            audit_logs_retain_days: policy.audit_logs_retain_days,
            deleted_message_purge_days: policy.deleted_message_purge_days,
        }
    }
}

impl From<RetentionRun> for RetentionRunResponse {
    fn from(run: RetentionRun) -> Self {
        Self {
            id: run.id.to_string(),
            organization_id: run.organization_id.to_string(),
            dry_run: run.dry_run,
            messages_purged: run.messages_purged,
            files_purged: run.files_purged,
            audit_events_purged: run.audit_events_purged,
            ran_at: run.ran_at,
        }
    }
}
