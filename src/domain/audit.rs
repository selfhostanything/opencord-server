use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::ids;

#[derive(Clone, Debug, PartialEq)]
pub struct AuditEvent {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub actor_user_id: Uuid,
    pub action: String,
    pub target_type: String,
    pub target_id: Uuid,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewAuditEvent {
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub actor_user_id: Uuid,
    pub action: &'static str,
    pub target_type: &'static str,
    pub target_id: Uuid,
    pub metadata: Value,
}

#[derive(Debug)]
pub enum AuditError {
    InvalidInput(&'static str),
    StoreUnavailable,
}

impl AuditError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::StoreUnavailable => "audit_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::StoreUnavailable => "audit store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait AuditStore: Send + Sync {
    async fn create_event(&self, event: AuditEvent) -> Result<(), AuditError>;
    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<AuditEvent>, AuditError>;
}

#[derive(Clone)]
pub struct AuditService {
    store: std::sync::Arc<dyn AuditStore>,
}

impl AuditService {
    pub fn new(store: std::sync::Arc<dyn AuditStore>) -> Self {
        Self { store }
    }

    pub async fn record(&self, input: NewAuditEvent) -> Result<AuditEvent, AuditError> {
        let event = AuditEvent {
            id: ids::new_uuid_v7(),
            organization_id: input.organization_id,
            space_id: input.space_id,
            actor_user_id: input.actor_user_id,
            action: normalize_label(input.action, "audit action is required")?,
            target_type: normalize_label(input.target_type, "audit target_type is required")?,
            target_id: input.target_id,
            metadata: input.metadata,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };

        self.store.create_event(event.clone()).await?;
        Ok(event)
    }

    pub async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<AuditEvent>, AuditError> {
        self.store.list_for_space(space_id).await
    }
}

fn normalize_label(value: &'static str, message: &'static str) -> Result<String, AuditError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 {
        Err(AuditError::InvalidInput(message))
    } else {
        Ok(value.to_owned())
    }
}
