use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};

use crate::domain::bot::{BotApplication, BotError, BotStore, StoredBotToken};

#[derive(Clone)]
pub struct PostgresBotStore {
    db: DatabaseConnection,
}

impl PostgresBotStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl BotStore for PostgresBotStore {
    async fn create_application(
        &self,
        application: BotApplication,
        token: StoredBotToken,
    ) -> Result<(), BotError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO bot_applications (
                    id, organization_id, bot_user_id, created_by_user_id, name, description, status
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7)
                "#,
                vec![
                    Value::from(application.id.to_string()),
                    Value::from(application.organization_id.to_string()),
                    Value::from(application.bot_user_id.to_string()),
                    Value::from(application.created_by_user_id.to_string()),
                    Value::from(application.name),
                    Value::from(application.description),
                    Value::from(application.status),
                ],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO bot_tokens (
                    id, application_id, token_hash, token_last_four, created_by_user_id, active
                )
                VALUES ($1::uuid, $2::uuid, $3, $4, $5::uuid, $6)
                "#,
                vec![
                    Value::from(token.id.to_string()),
                    Value::from(token.application_id.to_string()),
                    Value::from(token.token_hash),
                    Value::from(token.token_last_four),
                    Value::from(token.created_by_user_id.to_string()),
                    Value::from(token.active),
                ],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        Ok(())
    }
}
