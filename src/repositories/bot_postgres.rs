use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait, Value,
};

use uuid::Uuid;

use crate::domain::bot::{AuthenticatedBot, BotApplication, BotError, BotStore, StoredBotToken};

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

    async fn get_application(
        &self,
        application_id: Uuid,
    ) -> Result<Option<BotApplication>, BotError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, bot_user_id::text,
                       created_by_user_id::text, name, description, status
                FROM bot_applications
                WHERE id = $1::uuid
                "#,
                vec![Value::from(application_id.to_string())],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        row.map(application_from_row).transpose()
    }

    async fn list_applications(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<BotApplication>, BotError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, bot_user_id::text,
                       created_by_user_id::text, name, description, status
                FROM bot_applications
                WHERE organization_id = $1::uuid
                  AND status = 'active'
                ORDER BY name ASC, id ASC
                "#,
                vec![Value::from(organization_id.to_string())],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        rows.into_iter()
            .map(application_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn active_token_last_four(
        &self,
        application_id: Uuid,
    ) -> Result<Option<String>, BotError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT token_last_four
                FROM bot_tokens
                WHERE application_id = $1::uuid
                  AND active = true
                ORDER BY created_at DESC, id DESC
                LIMIT 1
                "#,
                vec![Value::from(application_id.to_string())],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        row.map(|row| {
            row.try_get::<String>("", "token_last_four")
                .map_err(|_| BotError::StoreUnavailable)
        })
        .transpose()
    }

    async fn rotate_token(&self, token: StoredBotToken) -> Result<(), BotError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
            UPDATE bot_tokens
            SET active = false,
                updated_at = now()
            WHERE application_id = $1::uuid
              AND active = true
            "#,
            vec![Value::from(token.application_id.to_string())],
        ))
        .await
        .map_err(|_| BotError::StoreUnavailable)?;

        txn.execute(Statement::from_sql_and_values(
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

        txn.commit().await.map_err(|_| BotError::StoreUnavailable)
    }

    async fn find_bot_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedBot>, BotError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT bot_applications.id::text AS application_id,
                       bot_applications.organization_id::text AS organization_id,
                       bot_applications.bot_user_id::text AS bot_user_id,
                       bot_applications.name AS name
                FROM bot_tokens
                INNER JOIN bot_applications
                  ON bot_applications.id = bot_tokens.application_id
                WHERE bot_tokens.token_hash = $1
                  AND bot_tokens.active = true
                  AND bot_applications.status = 'active'
                "#,
                vec![Value::from(token_hash.to_owned())],
            ))
            .await
            .map_err(|_| BotError::StoreUnavailable)?;

        row.map(bot_from_row).transpose()
    }
}

fn bot_from_row(row: sea_orm::QueryResult) -> Result<AuthenticatedBot, BotError> {
    Ok(AuthenticatedBot {
        application_id: parse_uuid(
            &row.try_get::<String>("", "application_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        bot_user_id: parse_uuid(
            &row.try_get::<String>("", "bot_user_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| BotError::StoreUnavailable)?,
    })
}

fn application_from_row(row: sea_orm::QueryResult) -> Result<BotApplication, BotError> {
    Ok(BotApplication {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        bot_user_id: parse_uuid(
            &row.try_get::<String>("", "bot_user_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        created_by_user_id: parse_uuid(
            &row.try_get::<String>("", "created_by_user_id")
                .map_err(|_| BotError::StoreUnavailable)?,
        )?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| BotError::StoreUnavailable)?,
        description: row
            .try_get::<Option<String>>("", "description")
            .map_err(|_| BotError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| BotError::StoreUnavailable)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, BotError> {
    Uuid::parse_str(value).map_err(|_| BotError::StoreUnavailable)
}
