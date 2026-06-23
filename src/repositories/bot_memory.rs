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

    async fn get_application(
        &self,
        application_id: Uuid,
    ) -> Result<Option<BotApplication>, BotError> {
        let state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        Ok(state.applications.get(&application_id).cloned())
    }

    async fn get_application_by_bot_user_id(
        &self,
        bot_user_id: Uuid,
    ) -> Result<Option<BotApplication>, BotError> {
        let state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        Ok(state
            .applications
            .values()
            .find(|application| application.bot_user_id == bot_user_id)
            .cloned())
    }

    async fn list_applications(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<BotApplication>, BotError> {
        let state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        let mut applications = state
            .applications
            .values()
            .filter(|application| {
                application.organization_id == organization_id && application.status == "active"
            })
            .cloned()
            .collect::<Vec<_>>();

        applications.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(applications)
    }

    async fn active_token_last_four(
        &self,
        application_id: Uuid,
    ) -> Result<Option<String>, BotError> {
        let state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        Ok(state
            .tokens
            .values()
            .find(|token| token.application_id == application_id && token.active)
            .map(|token| token.token_last_four.clone()))
    }

    async fn rotate_token(&self, token: StoredBotToken) -> Result<(), BotError> {
        let mut state = self.state.lock().map_err(|_| BotError::StoreUnavailable)?;
        if !state.applications.contains_key(&token.application_id) {
            return Err(BotError::NotFound);
        }

        for existing_token in state.tokens.values_mut() {
            if existing_token.application_id == token.application_id {
                existing_token.active = false;
            }
        }
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
