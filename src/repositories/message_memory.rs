use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::message::{Message, MessageError, MessageStore};

#[derive(Default)]
pub struct MemoryMessageStore {
    state: Mutex<MemoryMessageState>,
}

#[derive(Default)]
struct MemoryMessageState {
    messages_by_id: HashMap<Uuid, Message>,
}

#[async_trait::async_trait]
impl MessageStore for MemoryMessageStore {
    async fn create_message(&self, message: Message) -> Result<(), MessageError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        state.messages_by_id.insert(message.id, message);
        Ok(())
    }

    async fn list_for_channel(&self, channel_id: Uuid) -> Result<Vec<Message>, MessageError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        let mut messages = state
            .messages_by_id
            .values()
            .filter(|message| message.channel_id == channel_id && message.deleted_at.is_none())
            .cloned()
            .collect::<Vec<_>>();

        messages.sort_by_key(|message| message.id);
        Ok(messages)
    }

    async fn list_for_organization_between(
        &self,
        organization_id: Uuid,
        from: String,
        to: String,
    ) -> Result<Vec<Message>, MessageError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        let mut messages = state
            .messages_by_id
            .values()
            .filter(|message| {
                message.organization_id == organization_id
                    && message.created_at.as_str() >= from.as_str()
                    && message.created_at.as_str() <= to.as_str()
            })
            .cloned()
            .collect::<Vec<_>>();

        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(messages)
    }

    async fn get_message(&self, message_id: Uuid) -> Result<Option<Message>, MessageError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        Ok(state
            .messages_by_id
            .get(&message_id)
            .filter(|message| message.deleted_at.is_none())
            .cloned())
    }

    async fn update_message(&self, message: Message) -> Result<Message, MessageError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        if !state.messages_by_id.contains_key(&message.id) {
            return Err(MessageError::NotFound);
        }

        state.messages_by_id.insert(message.id, message.clone());
        Ok(message)
    }

    async fn delete_message(&self, message: Message) -> Result<(), MessageError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MessageError::StoreUnavailable)?;
        if !state.messages_by_id.contains_key(&message.id) {
            return Err(MessageError::NotFound);
        }

        state.messages_by_id.insert(message.id, message);
        Ok(())
    }
}
