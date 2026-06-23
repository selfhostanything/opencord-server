use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::command::{ApplicationCommand, CommandError, CommandInteraction, CommandStore};

#[derive(Clone)]
pub struct PostgresCommandStore {
    db: DatabaseConnection,
}

impl PostgresCommandStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl CommandStore for PostgresCommandStore {
    async fn create_command(
        &self,
        command: ApplicationCommand,
    ) -> Result<ApplicationCommand, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO application_commands (
                    id, application_id, organization_id, space_id, created_by_bot_user_id,
                    name, description, kind, options, status, created_at, updated_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                    $6, $7, $8, $9::jsonb, $10, $11::timestamptz, $12::timestamptz
                )
                RETURNING id::text, application_id::text, organization_id::text,
                          space_id::text, created_by_bot_user_id::text, name, description,
                          kind, options::text, status, created_at::text, updated_at::text
                "#,
                vec![
                    Value::from(command.id.to_string()),
                    Value::from(command.application_id.to_string()),
                    Value::from(command.organization_id.to_string()),
                    Value::from(command.space_id.map(|id| id.to_string())),
                    Value::from(command.created_by_bot_user_id.to_string()),
                    Value::from(command.name),
                    Value::from(command.description),
                    Value::from(command.kind),
                    Value::from(command.options.to_string()),
                    Value::from(command.status),
                    Value::from(command.created_at),
                    Value::from(command.updated_at),
                ],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(command_from_row)
            .transpose()?
            .ok_or(CommandError::StoreUnavailable)
    }

    async fn get_command(
        &self,
        command_id: Uuid,
    ) -> Result<Option<ApplicationCommand>, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                application_command_select_sql(
                    r#"
                    WHERE id = $1::uuid
                      AND status = 'active'
                    "#,
                ),
                vec![Value::from(command_id.to_string())],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(command_from_row).transpose()
    }

    async fn create_interaction(
        &self,
        interaction: CommandInteraction,
    ) -> Result<(), CommandError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO command_interactions (
                    id, application_id, organization_id, space_id, channel_id,
                    interaction_type, command_id, message_id, invoking_user_id,
                    token_hash, token_last_four, status, options, custom_id, component_type,
                    response_message_id, created_at, responded_at
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                    $6, $7::uuid, $8::uuid, $9::uuid, $10, $11, $12, $13::jsonb,
                    $14, $15, $16::uuid, $17::timestamptz, $18::timestamptz
                )
                "#,
                interaction_values(&interaction),
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        Ok(())
    }

    async fn get_interaction_by_token_hash(
        &self,
        interaction_id: Uuid,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                command_interaction_select_sql(
                    r#"
                    WHERE id = $1::uuid
                      AND token_hash = $2
                    "#,
                ),
                vec![
                    Value::from(interaction_id.to_string()),
                    Value::from(token_hash.to_owned()),
                ],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(interaction_from_row).transpose()
    }

    async fn find_interaction_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CommandInteraction>, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                command_interaction_select_sql(
                    r#"
                    WHERE token_hash = $1
                    "#,
                ),
                vec![Value::from(token_hash.to_owned())],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(interaction_from_row).transpose()
    }

    async fn mark_interaction_deferred(
        &self,
        interaction_id: Uuid,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE command_interactions
                SET status = 'deferred',
                    responded_at = $2::timestamptz
                WHERE id = $1::uuid
                  AND status = 'pending'
                RETURNING id::text, application_id::text, organization_id::text,
                          space_id::text, channel_id::text, interaction_type,
                          command_id::text, message_id::text, invoking_user_id::text,
                          token_hash, token_last_four, status, options::text,
                          custom_id, component_type, response_message_id::text,
                          created_at::text, responded_at::text
                "#,
                vec![
                    Value::from(interaction_id.to_string()),
                    Value::from(responded_at),
                ],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(interaction_from_row)
            .transpose()?
            .ok_or(CommandError::AlreadyResponded)
    }

    async fn mark_interaction_responded(
        &self,
        interaction_id: Uuid,
        response_message_id: Option<Uuid>,
        responded_at: String,
    ) -> Result<CommandInteraction, CommandError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE command_interactions
                SET status = 'responded',
                    response_message_id = COALESCE($2::uuid, response_message_id),
                    responded_at = $3::timestamptz
                WHERE id = $1::uuid
                  AND status IN ('pending', 'deferred')
                RETURNING id::text, application_id::text, organization_id::text,
                          space_id::text, channel_id::text, interaction_type,
                          command_id::text, message_id::text, invoking_user_id::text,
                          token_hash, token_last_four, status, options::text,
                          custom_id, component_type, response_message_id::text,
                          created_at::text, responded_at::text
                "#,
                vec![
                    Value::from(interaction_id.to_string()),
                    Value::from(response_message_id.map(|id| id.to_string())),
                    Value::from(responded_at),
                ],
            ))
            .await
            .map_err(|_| CommandError::StoreUnavailable)?;

        row.map(interaction_from_row)
            .transpose()?
            .ok_or(CommandError::AlreadyResponded)
    }
}

fn application_command_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, application_id::text, organization_id::text,
               space_id::text, created_by_bot_user_id::text, name, description,
               kind, options::text, status, created_at::text, updated_at::text
        FROM application_commands
        {where_clause}
        "#
    )
}

fn command_interaction_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, application_id::text, organization_id::text,
               space_id::text, channel_id::text, interaction_type,
               command_id::text, message_id::text, invoking_user_id::text,
               token_hash, token_last_four, status, options::text, custom_id,
               component_type, response_message_id::text, created_at::text,
               responded_at::text
        FROM command_interactions
        {where_clause}
        "#
    )
}

fn command_from_row(row: sea_orm::QueryResult) -> Result<ApplicationCommand, CommandError> {
    Ok(ApplicationCommand {
        id: parse_uuid(&row_string(&row, "id")?)?,
        application_id: parse_uuid(&row_string(&row, "application_id")?)?,
        organization_id: parse_uuid(&row_string(&row, "organization_id")?)?,
        space_id: parse_optional_uuid(row_optional_string(&row, "space_id")?)?,
        created_by_bot_user_id: parse_uuid(&row_string(&row, "created_by_bot_user_id")?)?,
        name: row_string(&row, "name")?,
        description: row_string(&row, "description")?,
        kind: row
            .try_get::<i32>("", "kind")
            .map_err(|_| CommandError::StoreUnavailable)?,
        options: parse_json(&row_string(&row, "options")?)?,
        status: row_string(&row, "status")?,
        created_at: row_string(&row, "created_at")?,
        updated_at: row_string(&row, "updated_at")?,
    })
}

fn interaction_from_row(row: sea_orm::QueryResult) -> Result<CommandInteraction, CommandError> {
    Ok(CommandInteraction {
        id: parse_uuid(&row_string(&row, "id")?)?,
        application_id: parse_uuid(&row_string(&row, "application_id")?)?,
        organization_id: parse_uuid(&row_string(&row, "organization_id")?)?,
        space_id: parse_uuid(&row_string(&row, "space_id")?)?,
        channel_id: parse_uuid(&row_string(&row, "channel_id")?)?,
        interaction_type: row
            .try_get::<i32>("", "interaction_type")
            .map_err(|_| CommandError::StoreUnavailable)?,
        command_id: parse_optional_uuid(row_optional_string(&row, "command_id")?)?,
        message_id: parse_optional_uuid(row_optional_string(&row, "message_id")?)?,
        invoking_user_id: parse_uuid(&row_string(&row, "invoking_user_id")?)?,
        token_hash: row_string(&row, "token_hash")?,
        token_last_four: row_string(&row, "token_last_four")?,
        status: row_string(&row, "status")?,
        options: parse_json(&row_string(&row, "options")?)?,
        custom_id: row_optional_string(&row, "custom_id")?,
        component_type: row
            .try_get::<Option<i32>>("", "component_type")
            .map_err(|_| CommandError::StoreUnavailable)?,
        response_message_id: parse_optional_uuid(row_optional_string(
            &row,
            "response_message_id",
        )?)?,
        created_at: row_string(&row, "created_at")?,
        responded_at: row
            .try_get::<Option<String>>("", "responded_at")
            .map_err(|_| CommandError::StoreUnavailable)?,
    })
}

fn interaction_values(interaction: &CommandInteraction) -> Vec<Value> {
    vec![
        Value::from(interaction.id.to_string()),
        Value::from(interaction.application_id.to_string()),
        Value::from(interaction.organization_id.to_string()),
        Value::from(interaction.space_id.to_string()),
        Value::from(interaction.channel_id.to_string()),
        Value::from(interaction.interaction_type),
        Value::from(interaction.command_id.map(|id| id.to_string())),
        Value::from(interaction.message_id.map(|id| id.to_string())),
        Value::from(interaction.invoking_user_id.to_string()),
        Value::from(interaction.token_hash.clone()),
        Value::from(interaction.token_last_four.clone()),
        Value::from(interaction.status.clone()),
        Value::from(interaction.options.to_string()),
        Value::from(interaction.custom_id.clone()),
        Value::from(interaction.component_type),
        Value::from(interaction.response_message_id.map(|id| id.to_string())),
        Value::from(interaction.created_at.clone()),
        Value::from(interaction.responded_at.clone()),
    ]
}

fn row_string(row: &sea_orm::QueryResult, column: &str) -> Result<String, CommandError> {
    row.try_get::<String>("", column)
        .map_err(|_| CommandError::StoreUnavailable)
}

fn row_optional_string(
    row: &sea_orm::QueryResult,
    column: &str,
) -> Result<Option<String>, CommandError> {
    row.try_get::<Option<String>>("", column)
        .map_err(|_| CommandError::StoreUnavailable)
}

fn parse_uuid(value: &str) -> Result<Uuid, CommandError> {
    Uuid::parse_str(value).map_err(|_| CommandError::StoreUnavailable)
}

fn parse_optional_uuid(value: Option<String>) -> Result<Option<Uuid>, CommandError> {
    value.as_deref().map(parse_uuid).transpose()
}

fn parse_json(value: &str) -> Result<serde_json::Value, CommandError> {
    serde_json::from_str(value).map_err(|_| CommandError::StoreUnavailable)
}
