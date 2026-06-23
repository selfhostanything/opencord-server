use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::bot::{AuthenticatedBot, BotApplication, BotError, BotStore, StoredBotToken};

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

    async fn find_bot_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedBot>, BotError> {
        let state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        let Some(token) = state
            .tokens
            .values()
            .find(|token| token.token_hash == token_hash && token.active)
        else {
            return Ok(None);
        };
        let Some(application) = state.applications.get(&token.application_id) else {
            return Ok(None);
        };
        if application.status != "active" {
            return Ok(None);
        }

        Ok(Some(AuthenticatedBot {
            application_id: application.id,
            organization_id: application.organization_id,
            bot_user_id: application.bot_user_id,
            name: application.name.clone(),
        }))
    }
}
