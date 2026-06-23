use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::command::{ApplicationCommand, CommandError, CommandInteraction, CommandStore};

#[derive(Default)]
pub struct MemoryCommandStore {
    state: Mutex<MemoryCommandState>,
}

#[derive(Default)]
struct MemoryCommandState {
    commands: HashMap<Uuid, ApplicationCommand>,
    interactions: HashMap<Uuid, CommandInteraction>,
}

#[async_trait::async_trait]
impl CommandStore for MemoryCommandStore {
    async fn create_command(
        &self,
        command: ApplicationCommand,
    ) -> Result<ApplicationCommand, CommandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        state.commands.insert(command.id, command.clone());
        Ok(command)
    }

    async fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<ApplicationCommand>, CommandError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        Ok(state
            .commands
            .get(&command_id)
            .filter(|command| command.status == "active")
            .cloned())
    }

    async fn create_interaction(
        &self,
        interaction: CommandInteraction,
    ) -> Result<(), CommandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        state.interactions.insert(interaction.id, interaction);
        Ok(())
    }

    async fn get_interaction_by_token_hash(
        &self,
        interaction_id: Uuid,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        Ok(state
            .interactions
            .get(&interaction_id)
            .filter(|interaction| interaction.token_hash == token_hash)
            .cloned())
    }

    async fn find_interaction_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        Ok(state
            .interactions
            .values()
            .find(|interaction| interaction.token_hash == token_hash)
            .cloned())
    }

    async fn mark_interaction_deferred(
        &self,
        interaction_id: Uuid,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        let interaction = state
            .interactions
            .get_mut(&interaction_id)
            .ok_or(CommandError::NotFound)?;
        if interaction.status != "pending" {
            return Err(CommandError::AlreadyResponded);
        }

        interaction.status = "deferred".to_owned();
        interaction.responded_at = Some(responded_at);
        Ok(interaction.clone())
    }

    async fn mark_interaction_responded(
        &self,
        interaction_id: Uuid,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CommandError::StoreUnavailable)?;
        let interaction = state
            .interactions
            .get_mut(&interaction_id)
            .ok_or(CommandError::NotFound)?;
        if interaction.status != "pending" && interaction.status != "deferred" {
            return Err(CommandError::AlreadyResponded);
        }

        interaction.status = "responded".to_owned();
        interaction.responded_at = Some(responded_at);
        Ok(interaction.clone())
    }
}
