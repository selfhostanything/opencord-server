use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::command::{ApplicationCommand, CommandInteractionCreated};

#[derive(Debug, Deserialize)]
pub struct CreateCompatApplicationCommandRequest {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: Option<i32>,
    pub options: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct CompatApplicationCommandResponse {
    pub id: String,
    pub application_id: String,
    pub guild_id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: i32,
    pub options: Value,
    pub version: String,
}

impl From<ApplicationCommand> for CompatApplicationCommandResponse {
    fn from(command: ApplicationCommand) -> Self {
        Self {
            id: command.id.to_string(),
            application_id: command.application_id.to_string(),
            guild_id: command.space_id.to_string(),
            name: command.name,
            description: command.description,
            kind: command.kind,
            options: command.options,
            version: command.id.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCommandInteractionRequest {
    pub command_id: Uuid,
    pub options: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct CommandInteractionCreatedResponse {
    pub interaction: CommandInteractionResponse,
}

#[derive(Debug, Serialize)]
pub struct CommandInteractionResponse {
    pub id: String,
    pub application_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub command_id: String,
    pub invoking_user_id: String,
    pub token: String,
    pub token_last_four: String,
    pub status: String,
    pub options: Value,
    pub created_at: String,
    pub responded_at: Option<String>,
}

impl From<CommandInteractionCreated> for CommandInteractionCreatedResponse {
    fn from(created: CommandInteractionCreated) -> Self {
        Self {
            interaction: CommandInteractionResponse {
                id: created.interaction.id.to_string(),
                application_id: created.interaction.application_id.to_string(),
                space_id: created.interaction.space_id.to_string(),
                channel_id: created.interaction.channel_id.to_string(),
                command_id: created.interaction.command_id.to_string(),
                invoking_user_id: created.interaction.invoking_user_id.to_string(),
                token: created.token,
                token_last_four: created.interaction.token_last_four,
                status: created.interaction.status,
                options: created.interaction.options,
                created_at: created.interaction.created_at,
                responded_at: created.interaction.responded_at,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInteractionCallbackRequest {
    #[serde(rename = "type")]
    pub kind: i32,
    pub data: Option<InteractionCallbackData>,
}

#[derive(Debug, Deserialize)]
pub struct InteractionCallbackData {
    pub content: String,
}
