use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::channel::{Channel, ChannelError, ChannelStore};

#[derive(Default)]
pub struct MemoryChannelStore {
    state: Mutex<MemoryChannelState>,
}

#[derive(Default)]
struct MemoryChannelState {
    channels_by_id: HashMap<Uuid, Channel>,
    channel_id_by_space_slug: HashMap<(Uuid, String), Uuid>,
}

#[async_trait::async_trait]
impl ChannelStore for MemoryChannelStore {
    async fn create_channel(&self, channel: Channel) -> Result<(), ChannelError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ChannelError::StoreUnavailable)?;
        let key = (channel.space_id, channel.slug.clone());

        if state.channel_id_by_space_slug.contains_key(&key) {
            return Err(ChannelError::SlugAlreadyExists);
        }

        state.channel_id_by_space_slug.insert(key, channel.id);
        state.channels_by_id.insert(channel.id, channel);

        Ok(())
    }

    async fn list_for_space(&self, space_id: Uuid) -> Result<Vec<Channel>, ChannelError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ChannelError::StoreUnavailable)?;
        let mut channels = state
            .channels_by_id
            .values()
            .filter(|channel| channel.space_id == space_id)
            .cloned()
            .collect::<Vec<_>>();

        channels.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(channels)
    }

    async fn get_channel(&self, channel_id: Uuid) -> Result<Option<Channel>, ChannelError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ChannelError::StoreUnavailable)?;
        Ok(state.channels_by_id.get(&channel_id).cloned())
    }

    async fn update_channel(&self, channel: Channel) -> Result<Channel, ChannelError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ChannelError::StoreUnavailable)?;
        let Some(previous) = state.channels_by_id.get(&channel.id).cloned() else {
            return Err(ChannelError::NotFound);
        };
        let next_key = (channel.space_id, channel.slug.clone());

        if state
            .channel_id_by_space_slug
            .get(&next_key)
            .is_some_and(|existing_id| *existing_id != channel.id)
        {
            return Err(ChannelError::SlugAlreadyExists);
        }

        state
            .channel_id_by_space_slug
            .remove(&(previous.space_id, previous.slug));
        state.channel_id_by_space_slug.insert(next_key, channel.id);
        state.channels_by_id.insert(channel.id, channel.clone());

        Ok(channel)
    }
}
