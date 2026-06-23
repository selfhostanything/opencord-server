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

#[derive(Debug, Deserialize)]
pub struct CreateComponentInteractionRequest {
    pub message_id: Uuid,
    pub custom_id: String,
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
    #[serde(rename = "type")]
    pub kind: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub invoking_user_id: String,
    pub token: String,
    pub token_last_four: String,
    pub status: String,
    pub options: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_id: Option<String>,
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
                kind: created.interaction.interaction_type,
                command_id: created.interaction.command_id.map(|id| id.to_string()),
                message_id: created.interaction.message_id.map(|id| id.to_string()),
                invoking_user_id: created.interaction.invoking_user_id.to_string(),
                token: created.token,
                token_last_four: created.interaction.token_last_four,
                status: created.interaction.status,
                options: created.interaction.options,
                custom_id: created.interaction.custom_id,
                component_type: created.interaction.component_type,
                response_message_id: created
                    .interaction
                    .response_message_id
                    .map(|id| id.to_string()),
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

#[derive(Debug, Deserialize)]
pub struct CreateInteractionFollowupRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchInteractionOriginalResponseRequest {
    pub content: String,
}
