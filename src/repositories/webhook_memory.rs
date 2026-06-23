use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::webhook::{IncomingWebhook, IncomingWebhookStore, WebhookError};

#[derive(Default)]
pub struct MemoryIncomingWebhookStore {
    state: Mutex<MemoryIncomingWebhookState>,
}

#[derive(Default)]
struct MemoryIncomingWebhookState {
    webhooks: HashMap<Uuid, IncomingWebhook>,
}

#[async_trait::async_trait]
impl IncomingWebhookStore for MemoryIncomingWebhookStore {
    async fn create_webhook(&self, webhook: IncomingWebhook) -> Result<(), WebhookError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WebhookError::StoreUnavailable)?;
        state.webhooks.insert(webhook.id, webhook);
        Ok(())
    }

    async fn get_webhook(&self, webhook_id: Uuid) -> Result<Option<IncomingWebhook>, WebhookError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WebhookError::StoreUnavailable)?;
        Ok(state.webhooks.get(&webhook_id).cloned())
    }
}
