use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::bot::{BotApplication, BotError, BotStore, StoredBotToken};

#[derive(Default)]
pub struct MemoryBotStore {
    state: Mutex<MemoryBotState>,
}

#[derive(Default)]
struct MemoryBotState {
    applications: HashMap<Uuid, BotApplication>,
    tokens: HashMap<Uuid, StoredBotToken>,
}

#[async_trait::async_trait]
impl BotStore for MemoryBotStore {
    async fn create_application(
        &self,
        application: BotApplication,
        token: StoredBotToken,
    ) -> Result<(), BotError> {
        let mut state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        state.applications.insert(application.id, application);
        state.tokens.insert(token.id, token);
        Ok(())
    }
}
