use serde::{Deserialize, Serialize};

use crate::domain::retention::RetentionPolicy;

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
pub struct RetentionPolicyResponse {
    pub organization_id: String,
    pub messages_retain_days: Option<i64>,
    pub files_retain_days: Option<i64>,
    pub audit_logs_retain_days: Option<i64>,
    pub deleted_message_purge_days: Option<i64>,
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
