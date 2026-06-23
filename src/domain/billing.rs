use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::ids;
use crate::domain::organization::OrganizationStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingState {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub provider: String,
    pub event_type: String,
    pub external_customer_id: String,
    pub external_subscription_id: String,
    pub plan: String,
    pub status: String,
    pub current_period_end: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingProviderEvent {
    pub organization_id: Uuid,
    pub provider: String,
    pub event_type: String,
    pub external_customer_id: String,
    pub external_subscription_id: String,
    pub plan: String,
    pub status: String,
    pub current_period_end: Option<String>,
}

#[derive(Debug)]
pub enum BillingError {
    InvalidInput(&'static str),
    OrganizationNotFound,
    StoreUnavailable,
}

impl BillingError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::OrganizationNotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::OrganizationNotFound => "organization_not_found",
            Self::StoreUnavailable => "billing_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::OrganizationNotFound => "organization was not found",
            Self::StoreUnavailable => "billing store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait BillingStore: Send + Sync {
    async fn upsert_state(&self, state: BillingState) -> Result<BillingState, BillingError>;
}

#[derive(Clone)]
pub struct BillingService {
    store: std::sync::Arc<dyn BillingStore>,
    organizations: std::sync::Arc<dyn OrganizationStore>,
}

impl BillingService {
    pub fn new(
        store: std::sync::Arc<dyn BillingStore>,
        organizations: std::sync::Arc<dyn OrganizationStore>,
    ) -> Self {
        Self {
            store,
            organizations,
        }
    }

    pub async fn apply_provider_event(
        &self,
        event: BillingProviderEvent,
    ) -> Result<BillingState, BillingError> {
        let provider = normalize_provider(event.provider)?;
        let event_type = normalize_event_type(event.event_type)?;
        let plan = normalize_plan(event.plan)?;
        let status = normalize_status(event.status)?;
        let current_period_end = event.current_period_end.map(normalize_time).transpose()?;
        let external_customer_id = normalize_required(
            event.external_customer_id,
            "billing external_customer_id is required",
            256,
        )?;
        let external_subscription_id = normalize_required(
            event.external_subscription_id,
            "billing external_subscription_id is required",
            256,
        )?;

        self.organizations
            .update_plan(event.organization_id, plan.clone())
            .await
            .map_err(|error| match error {
                crate::domain::organization::OrganizationError::NotFound => {
                    BillingError::OrganizationNotFound
                }
                _ => BillingError::StoreUnavailable,
            })?;

        let now = now_string();
        self.store
            .upsert_state(BillingState {
                id: ids::new_uuid_v7(),
                organization_id: event.organization_id,
                provider,
                event_type,
                external_customer_id,
                external_subscription_id,
                plan,
                status,
                current_period_end,
                updated_at: now,
            })
            .await
    }
}

fn normalize_provider(provider: String) -> Result<String, BillingError> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "stripe" => Ok("stripe".to_owned()),
        _ => Err(BillingError::InvalidInput(
            "billing provider must be stripe",
        )),
    }
}

fn normalize_event_type(event_type: String) -> Result<String, BillingError> {
    match event_type.trim().to_ascii_lowercase().as_str() {
        "subscription.created"
        | "subscription.updated"
        | "subscription.cancelled"
        | "invoice.payment_failed"
        | "invoice.paid"
        | "plan.changed" => Ok(event_type.trim().to_ascii_lowercase()),
        _ => Err(BillingError::InvalidInput(
            "billing event_type is not supported",
        )),
    }
}

fn normalize_plan(plan: String) -> Result<String, BillingError> {
    match plan.trim().to_ascii_lowercase().as_str() {
        "free" | "team" | "business" | "enterprise" => Ok(plan.trim().to_ascii_lowercase()),
        _ => Err(BillingError::InvalidInput(
            "billing plan must be free, team, business, or enterprise",
        )),
    }
}

fn normalize_status(status: String) -> Result<String, BillingError> {
    match status.trim().to_ascii_lowercase().as_str() {
        "active" | "trialing" | "past_due" | "cancelled" | "unpaid" => {
            Ok(status.trim().to_ascii_lowercase())
        }
        _ => Err(BillingError::InvalidInput(
            "billing status is not supported",
        )),
    }
}

fn normalize_required(
    value: String,
    message: &'static str,
    max_len: usize,
) -> Result<String, BillingError> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if (1..=max_len).contains(&value.len()) {
        Ok(value)
    } else {
        Err(BillingError::InvalidInput(message))
    }
}

fn normalize_time(value: String) -> Result<String, BillingError> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|_| BillingError::InvalidInput("billing current_period_end must be RFC3339"))
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
