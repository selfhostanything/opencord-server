use serde::Serialize;
use serde_json::Value;

use crate::domain::audit::AuditEvent;

#[derive(Clone, Debug, Serialize)]
pub struct AuditEventResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub actor_user_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct AuditEventListResponse {
    pub audit_events: Vec<AuditEventResponse>,
}

impl From<AuditEvent> for AuditEventResponse {
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.id.to_string(),
            organization_id: event.organization_id.to_string(),
            space_id: event.space_id.to_string(),
            actor_user_id: event.actor_user_id.to_string(),
            action: event.action,
            target_type: event.target_type,
            target_id: event.target_id.to_string(),
            metadata: event.metadata,
            created_at: event.created_at,
        }
    }
}
