use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::billing::{BillingProviderEvent, BillingState};

#[derive(Debug, Deserialize)]
pub struct BillingProviderEventRequest {
    pub organization_id: Uuid,
    pub provider: String,
    pub event_type: String,
    pub external_customer_id: String,
    pub external_subscription_id: String,
    pub plan: String,
    pub status: String,
    pub current_period_end: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BillingStateResponse {
    pub id: String,
    pub organization_id: String,
    pub provider: String,
    pub event_type: String,
    pub external_customer_id: String,
    pub external_subscription_id: String,
    pub plan: String,
    pub status: String,
    pub current_period_end: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BillingStateResourceResponse {
    pub billing: BillingStateResponse,
}

impl From<BillingProviderEventRequest> for BillingProviderEvent {
    fn from(request: BillingProviderEventRequest) -> Self {
        Self {
            organization_id: request.organization_id,
            provider: request.provider,
            event_type: request.event_type,
            external_customer_id: request.external_customer_id,
            external_subscription_id: request.external_subscription_id,
            plan: request.plan,
            status: request.status,
            current_period_end: request.current_period_end,
        }
    }
}

impl From<BillingState> for BillingStateResponse {
    fn from(state: BillingState) -> Self {
        Self {
            id: state.id.to_string(),
            organization_id: state.organization_id.to_string(),
            provider: state.provider,
            event_type: state.event_type,
            external_customer_id: state.external_customer_id,
            external_subscription_id: state.external_subscription_id,
            plan: state.plan,
            status: state.status,
            current_period_end: state.current_period_end,
            updated_at: state.updated_at,
        }
    }
}
