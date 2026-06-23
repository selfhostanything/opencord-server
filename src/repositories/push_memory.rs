use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::push::{PushError, PushToken, PushTokenStore};

#[derive(Default)]
pub struct MemoryPushTokenStore {
    state: Mutex<MemoryPushTokenState>,
}

#[derive(Default)]
struct MemoryPushTokenState {
    tokens_by_key: HashMap<PushTokenKey, PushToken>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PushTokenKey {
    user_id: Uuid,
    platform: String,
    token_hash: String,
}

#[async_trait::async_trait]
impl PushTokenStore for MemoryPushTokenStore {
    async fn upsert_token(&self, token: PushToken) -> Result<PushToken, PushError> {
        let mut state = self.state.lock().map_err(|_| PushError::StoreUnavailable)?;
        let key = PushTokenKey::from(&token);
        let token = match state.tokens_by_key.get(&key) {
            Some(existing) => PushToken {
                id: existing.id,
                created_at: existing.created_at.clone(),
                ..token
            },
            None => token,
        };

        state.tokens_by_key.insert(key, token.clone());
        Ok(token)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<PushToken>, PushError> {
        let state = self.state.lock().map_err(|_| PushError::StoreUnavailable)?;
        let mut tokens = state
            .tokens_by_key
            .values()
            .filter(|token| token.user_id == user_id)
            .cloned()
            .collect::<Vec<_>>();
        tokens.sort_by_key(|token| token.id);
        Ok(tokens)
    }
}

impl From<&PushToken> for PushTokenKey {
    fn from(token: &PushToken) -> Self {
        Self {
            user_id: token.user_id,
            platform: token.platform.as_str().to_owned(),
            token_hash: token.token_hash.clone(),
        }
    }
}
