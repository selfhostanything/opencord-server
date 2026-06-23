use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::bot::AuthenticatedBot;
use crate::domain::ids;

pub const INTERACTION_TYPE_APPLICATION_COMMAND: i32 = 2;
pub const INTERACTION_TYPE_MESSAGE_COMPONENT: i32 = 3;
pub const INTERACTION_CALLBACK_CHANNEL_MESSAGE: i32 = 4;
pub const INTERACTION_CALLBACK_DEFERRED_CHANNEL_MESSAGE: i32 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationCommand {
    pub id: Uuid,
    pub application_id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub created_by_bot_user_id: Uuid,
    pub name: String,
    pub description: String,
    pub kind: i32,
    pub options: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandInteraction {
    pub id: Uuid,
    pub application_id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub interaction_type: i32,
    pub command_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub invoking_user_id: Uuid,
    pub token_hash: String,
    pub token_last_four: String,
    pub status: String,
    pub options: Value,
    pub custom_id: Option<String>,
    pub component_type: Option<i32>,
    pub response_message_id: Option<Uuid>,
    pub created_at: String,
    pub responded_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandInteractionCreated {
    pub interaction: CommandInteraction,
    pub token: String,
}

#[derive(Debug)]
pub struct CreateApplicationCommandInput {
    pub bot: AuthenticatedBot,
    pub space_id: Uuid,
    pub name: String,
    pub description: String,
    pub kind: Option<i32>,
    pub options: Option<Value>,
}

#[derive(Debug)]
pub struct CreateCommandInteractionInput {
    pub command_id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub invoking_user_id: Uuid,
    pub options: Option<Value>,
}

#[derive(Debug)]
pub struct CreateComponentInteractionInput {
    pub application_id: Uuid,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub message_id: Uuid,
    pub invoking_user_id: Uuid,
    pub custom_id: String,
    pub component_type: i32,
}

#[derive(Debug)]
pub enum CommandError {
    InvalidInput(&'static str),
    Unauthorized,
    NotFound,
    AlreadyResponded,
    StoreUnavailable,
}

impl CommandError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::AlreadyResponded => StatusCode::CONFLICT,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "command_not_found",
            Self::AlreadyResponded => "interaction_already_responded",
            Self::StoreUnavailable => "command_store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::Unauthorized => "valid interaction token is required",
            Self::NotFound => "command or interaction was not found",
            Self::AlreadyResponded => "interaction has already been responded to",
            Self::StoreUnavailable => "command store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait CommandStore: Send + Sync {
    async fn create_command(
        &self,
        command: ApplicationCommand,
    ) -> Result<ApplicationCommand, CommandError>;
    async fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<ApplicationCommand>, CommandError>;
    async fn create_interaction(&self, interaction: CommandInteraction)
    -> Result<(), CommandError>;
    async fn get_interaction_by_token_hash(
        &self,
        interaction_id: Uuid,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError>;
    async fn find_interaction_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError>;
    async fn mark_interaction_deferred(
        &self,
        interaction_id: Uuid,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError>;
    async fn mark_interaction_responded(
        &self,
        interaction_id: Uuid,
        response_message_id: Option<Uuid>,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError>;
}

#[derive(Clone)]
pub struct CommandService {
    store: std::sync::Arc<dyn CommandStore>,
}

impl CommandService {
    pub fn new(store: std::sync::Arc<dyn CommandStore>) -> Self {
        Self { store }
    }

    pub async fn create_space_command(
        &self,
        input: CreateApplicationCommandInput,
    ) -> Result<ApplicationCommand, CommandError> {
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let command = ApplicationCommand {
            id: ids::new_uuid_v7(),
            application_id: input.bot.application_id,
            organization_id: input.bot.organization_id,
            space_id: input.space_id,
            created_by_bot_user_id: input.bot.bot_user_id,
            name: normalize_command_name(input.name)?,
            description: normalize_description(input.description)?,
            kind: normalize_command_kind(input.kind)?,
            options: normalize_options(input.options)?,
            status: "active".to_owned(),
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.create_command(command).await
    }

    pub async fn get_command(&self, command_id: Uuid) -> Result<ApplicationCommand, CommandError> {
        self.store
            .get_command(command_id)
            .await?
            .ok_or(CommandError::NotFound)
    }

    pub async fn create_interaction(
        &self,
        input: CreateCommandInteractionInput,
    ) -> Result<CommandInteractionCreated, CommandError> {
        let command = self.get_command(input.command_id).await?;
        if command.organization_id != input.organization_id || command.space_id != input.space_id {
            return Err(CommandError::NotFound);
        }

        let token = generate_interaction_token();
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let interaction = CommandInteraction {
            id: ids::new_uuid_v7(),
            application_id: command.application_id,
            organization_id: input.organization_id,
            space_id: input.space_id,
            channel_id: input.channel_id,
            interaction_type: INTERACTION_TYPE_APPLICATION_COMMAND,
            command_id: Some(command.id),
            message_id: None,
            invoking_user_id: input.invoking_user_id,
            token_hash: hash_interaction_token(&token),
            token_last_four: token_last_four(&token),
            status: "pending".to_owned(),
            options: normalize_interaction_options(input.options)?,
            custom_id: None,
            component_type: None,
            response_message_id: None,
            created_at: now,
            responded_at: None,
        };

        self.store.create_interaction(interaction.clone()).await?;

        Ok(CommandInteractionCreated { interaction, token })
    }

    pub async fn create_component_interaction(
        &self,
        input: CreateComponentInteractionInput,
    ) -> Result<CommandInteractionCreated, CommandError> {
        let token = generate_interaction_token();
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let interaction = CommandInteraction {
            id: ids::new_uuid_v7(),
            application_id: input.application_id,
            organization_id: input.organization_id,
            space_id: input.space_id,
            channel_id: input.channel_id,
            interaction_type: INTERACTION_TYPE_MESSAGE_COMPONENT,
            command_id: None,
            message_id: Some(input.message_id),
            invoking_user_id: input.invoking_user_id,
            token_hash: hash_interaction_token(&token),
            token_last_four: token_last_four(&token),
            status: "pending".to_owned(),
            options: Value::Array(Vec::new()),
            custom_id: Some(normalize_component_custom_id(input.custom_id)?),
            component_type: Some(normalize_component_type(input.component_type)?),
            response_message_id: None,
            created_at: now,
            responded_at: None,
        };

        self.store.create_interaction(interaction.clone()).await?;

        Ok(CommandInteractionCreated { interaction, token })
    }

    pub async fn interaction_for_callback(
        &self,
        interaction_id: Uuid,
        token: &str,
    ) -> Result<CommandInteraction, CommandError> {
        if !token.starts_with("oci_") {
            return Err(CommandError::Unauthorized);
        }

        let interaction = self
            .store
            .get_interaction_by_token_hash(interaction_id, &hash_interaction_token(token))
            .await?
            .ok_or(CommandError::Unauthorized)?;

        if interaction.status != "pending" {
            return Err(CommandError::AlreadyResponded);
        }

        Ok(interaction)
    }

    pub async fn interaction_for_followup(
        &self,
        application_id: Uuid,
        token: &str,
    ) -> Result<CommandInteraction, CommandError> {
        if !token.starts_with("oci_") {
            return Err(CommandError::Unauthorized);
        }

        let interaction = self
            .store
            .find_interaction_by_token_hash(&hash_interaction_token(token))
            .await?
            .ok_or(CommandError::Unauthorized)?;

        if interaction.application_id != application_id {
            return Err(CommandError::Unauthorized);
        }

        if interaction.status != "deferred" {
            return Err(CommandError::AlreadyResponded);
        }

        Ok(interaction)
    }

    pub async fn interaction_for_original_response(
        &self,
        application_id: Uuid,
        token: &str,
    ) -> Result<CommandInteraction, CommandError> {
        if !token.starts_with("oci_") {
            return Err(CommandError::Unauthorized);
        }

        let interaction = self
            .store
            .find_interaction_by_token_hash(&hash_interaction_token(token))
            .await?
            .ok_or(CommandError::Unauthorized)?;

        if interaction.application_id != application_id {
            return Err(CommandError::Unauthorized);
        }

        if interaction.status != "responded" || interaction.response_message_id.is_none() {
            return Err(CommandError::NotFound);
        }

        Ok(interaction)
    }

    pub async fn mark_interaction_deferred(
        &self,
        interaction_id: Uuid,
    ) -> Result<CommandInteraction, CommandError> {
        self.store
            .mark_interaction_deferred(
                interaction_id,
                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            )
            .await
    }

    pub async fn mark_interaction_responded(
        &self,
        interaction_id: Uuid,
        response_message_id: Option<Uuid>,
    ) -> Result<CommandInteraction, CommandError> {
        self.store
            .mark_interaction_responded(
                interaction_id,
                response_message_id,
                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            )
            .await
    }
}

fn normalize_command_name(name: String) -> Result<String, CommandError> {
    let name = name.trim().to_ascii_lowercase();
    if !(1..=32).contains(&name.len()) {
        return Err(CommandError::InvalidInput(
            "command name must be 1 to 32 characters",
        ));
    }
    if !name.chars().all(|char| {
        char.is_ascii_lowercase() || char.is_ascii_digit() || char == '_' || char == '-'
    }) {
        return Err(CommandError::InvalidInput(
            "command name must contain lowercase letters, numbers, underscores, or hyphens",
        ));
    }

    Ok(name)
}

fn normalize_description(description: String) -> Result<String, CommandError> {
    let description = description.trim().to_owned();
    if (1..=100).contains(&description.len()) {
        Ok(description)
    } else {
        Err(CommandError::InvalidInput(
            "command description must be 1 to 100 characters",
        ))
    }
}

fn normalize_command_kind(kind: Option<i32>) -> Result<i32, CommandError> {
    match kind.unwrap_or(1) {
        1 => Ok(1),
        _ => Err(CommandError::InvalidInput(
            "only chat input commands are supported",
        )),
    }
}

fn normalize_options(options: Option<Value>) -> Result<Value, CommandError> {
    let options = options.unwrap_or_else(|| Value::Array(Vec::new()));
    match options.as_array() {
        Some(values) if values.len() <= 25 => Ok(options),
        Some(_) => Err(CommandError::InvalidInput(
            "command options cannot contain more than 25 entries",
        )),
        None => Err(CommandError::InvalidInput(
            "command options must be an array",
        )),
    }
}

fn normalize_interaction_options(options: Option<Value>) -> Result<Value, CommandError> {
    let options = options.unwrap_or_else(|| Value::Array(Vec::new()));
    if options.is_array() {
        Ok(options)
    } else {
        Err(CommandError::InvalidInput(
            "interaction options must be an array",
        ))
    }
}

fn normalize_component_custom_id(custom_id: String) -> Result<String, CommandError> {
    let custom_id = custom_id.trim().to_owned();
    if (1..=100).contains(&custom_id.len()) {
        Ok(custom_id)
    } else {
        Err(CommandError::InvalidInput(
            "component custom_id must be 1 to 100 characters",
        ))
    }
}

fn normalize_component_type(component_type: i32) -> Result<i32, CommandError> {
    match component_type {
        2..=8 => Ok(component_type),
        _ => Err(CommandError::InvalidInput(
            "component type must be a Discord message component type",
        )),
    }
}

fn generate_interaction_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("oci_{}", hex::encode(bytes))
}

fn hash_interaction_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn token_last_four(token: &str) -> String {
    token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
