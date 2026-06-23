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

    async fn list_webhooks_for_channel(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<IncomingWebhook>, WebhookError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WebhookError::StoreUnavailable)?;
        let mut webhooks = state
            .webhooks
            .values()
            .filter(|webhook| webhook.channel_id == channel_id && webhook.status == "active")
            .cloned()
            .collect::<Vec<_>>();

        webhooks.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(webhooks)
    }

    async fn rotate_webhook_token(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
        token_hash: String,
        token_last_four: String,
    ) -> Result<Option<IncomingWebhook>, WebhookError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WebhookError::StoreUnavailable)?;
        let Some(webhook) = state.webhooks.get_mut(&webhook_id) else {
            return Ok(None);
        };

        if webhook.channel_id != channel_id || webhook.status != "active" {
            return Ok(None);
        }

        webhook.token_hash = token_hash;
        webhook.token_last_four = token_last_four;
        Ok(Some(webhook.clone()))
    }

    async fn disable_webhook(
        &self,
        webhook_id: Uuid,
        channel_id: Uuid,
    ) -> Result<bool, WebhookError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WebhookError::StoreUnavailable)?;
        let Some(webhook) = state.webhooks.get_mut(&webhook_id) else {
            return Ok(false);
        };

        if webhook.channel_id != channel_id || webhook.status != "active" {
            return Ok(false);
        }

        webhook.status = "disabled".to_owned();
        Ok(true)
    }
}
